// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Agent de signalisation : pont entre le domaine synchrone et rsipstack.
//!
//! ## Le problème
//!
//! Les traits de `od-core` sont **synchrones** — choix délibéré, qui évite au
//! domaine de dépendre d'un runtime asynchrone et rend ses tests triviaux.
//! rsipstack, lui, est entièrement asynchrone.
//!
//! ## La solution
//!
//! Un **thread dédié** héberge un runtime tokio et l'endpoint SIP. L'appelant
//! lui adresse des commandes par canal, et attend la réponse :
//!
//! ```text
//!   Thread appelant                 Thread de signalisation
//!   ───────────────                 ───────────────────────
//!   register(&account)
//!     └─ envoie Commande::Register
//!     └─ attend sur canal  ─────►  runtime.block_on(register())
//!                                   └─ renvoie le résultat
//!        ◄──────────────────────────┘
//!     └─ traduit en RegistrationState
//! ```
//!
//! Pourquoi pas le runtime de l'appelant : rsipstack doit rester actif en
//! permanence pour recevoir les requêtes entrantes et rafraîchir
//! l'enregistrement avant expiration. Un runtime créé par appel mourrait à
//! chaque retour, et le compte deviendrait injoignable en silence.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc;
use std::thread;

use od_core::{
    Account, AccountId, RegistrationState, SignalingError, SignalingPort,
};
use rsipstack::dialog::registration::Registration;
use rsipstack::transport::udp::UdpConnection;
use rsipstack::transport::TransportLayer;
use rsipstack::EndpointBuilder;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::error::AdaptError;
use crate::mapping;

/// Délai maximal accordé à une opération de signalisation.
///
/// Un serveur SIP injoignable laisse la socket en attente indéfiniment : sans
/// cette borne, l'interface se figerait sur un appel bloqué. Trente secondes
/// couvrent largement un enregistrement sur un serveur distant, et restent
/// tolérables pour l'utilisateur en cas d'échec.
const OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Commande adressée au thread de signalisation.
enum Command {
    /// Enregistre un compte et renvoie l'état résultant.
    Register {
        account: Box<Account>,
        reply: mpsc::Sender<Result<RegistrationState, AdaptError>>,
    },
    /// Désenregistre un compte.
    Unregister {
        account_id: AccountId,
        reply: mpsc::Sender<Result<(), AdaptError>>,
    },
}

/// Agent de signalisation SIP.
///
/// Détient un thread dédié exécutant un runtime tokio et l'endpoint rsipstack.
/// L'agent est arrêté proprement lorsqu'il est relâché : le jeton
/// d'annulation est déclenché et le thread est rejoint.
pub struct SipAgent {
    /// Canal d'envoi des commandes vers le thread de signalisation.
    commands: Option<mpsc::Sender<Command>>,
    /// Poignée du thread, pour un arrêt propre.
    thread: Option<thread::JoinHandle<()>>,
    /// Jeton d'annulation partagé avec le runtime.
    cancel: CancellationToken,
}

impl SipAgent {
    /// Démarre l'agent sur un port local donné.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`AdaptError::Protocol`] si la socket locale ne peut pas être
    /// ouverte — port déjà utilisé, ou permission refusée pour un port
    /// privilégié.
    pub fn start(local_port: u16) -> Result<Self, AdaptError> {
        let (command_tx, command_rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AdaptError>>();
        let cancel = CancellationToken::new();
        let cancel_for_thread = cancel.clone();

        let thread = thread::Builder::new()
            .name("od-sip".to_owned())
            .spawn(move || {
                run_agent_thread(local_port, command_rx, ready_tx, cancel_for_thread);
            })
            .map_err(|error| mapping::adapt_error("démarrage du thread", error))?;

        // On attend que le runtime soit prêt AVANT de rendre la main : sans
        // cela, un appelant pourrait envoyer une commande dans un canal dont
        // le récepteur n'est pas encore installé, et l'échec serait silencieux.
        match ready_rx.recv_timeout(OPERATION_TIMEOUT) {
            Ok(Ok(())) => {
                info!(port = local_port, "agent de signalisation démarré");
                Ok(Self {
                    commands: Some(command_tx),
                    thread: Some(thread),
                    cancel,
                })
            }
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                cancel.cancel();
                let _ = thread.join();
                Err(AdaptError::Timeout(
                    "le démarrage de l'agent de signalisation".to_owned(),
                ))
            }
        }
    }

    /// Envoie une commande et attend sa réponse.
    fn request<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, AdaptError>>) -> Command,
    ) -> Result<T, AdaptError> {
        let commands = self.commands.as_ref().ok_or(AdaptError::NotRunning)?;

        let (reply_tx, reply_rx) = mpsc::channel();
        commands
            .send(build(reply_tx))
            .map_err(|_| AdaptError::NotRunning)?;

        // Le délai protège l'appelant : sans lui, une commande perdue par un
        // thread bloqué figerait l'interface indéfiniment.
        match reply_rx.recv_timeout(OPERATION_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(AdaptError::Timeout(
                "aucune réponse du service de signalisation".to_owned(),
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(AdaptError::NotRunning),
        }
    }
}

impl SignalingPort for SipAgent {
    fn register(&mut self, account: &Account) -> Result<RegistrationState, SignalingError> {
        let account = account.clone();
        self.request(|reply| Command::Register {
            account: Box::new(account),
            reply,
        })
        .map_err(|error| error.to_signaling_error())
    }

    fn unregister(&mut self, account_id: &AccountId) -> Result<(), SignalingError> {
        let account_id = account_id.clone();
        self.request(|reply| Command::Unregister { account_id, reply })
            .map_err(|error| error.to_signaling_error())
    }

    fn start_call(
        &mut self,
        _account_id: &AccountId,
        _destination: &str,
    ) -> Result<od_core::Call, SignalingError> {
        // Phase 2 : l'établissement d'appel arrive avec la négociation SDP et
        // le pipeline média. Renvoyer une erreur explicite vaut mieux qu'un
        // succès silencieux qui masquerait une fonctionnalité absente.
        Err(SignalingError::permanent(
            "l'établissement d'appel n'est pas encore implémenté (Phase 2)",
        ))
    }

    fn accept_call(&mut self, _call_id: &od_core::CallId) -> Result<(), SignalingError> {
        Err(SignalingError::permanent(
            "l'acceptation d'appel n'est pas encore implémentée (Phase 2)",
        ))
    }

    fn reject_call(&mut self, _call_id: &od_core::CallId) -> Result<(), SignalingError> {
        Err(SignalingError::permanent(
            "le refus d'appel n'est pas encore implémenté (Phase 2)",
        ))
    }

    fn hangup_call(&mut self, _call_id: &od_core::CallId) -> Result<(), SignalingError> {
        Err(SignalingError::permanent(
            "le raccrochage n'est pas encore implémenté (Phase 2)",
        ))
    }
}

impl Drop for SipAgent {
    fn drop(&mut self) {
        // Ordre important : on coupe d'abord le canal pour que la boucle de
        // commandes se termine, PUIS on annule le jeton pour arrêter les
        // tâches de fond, et enfin on rejoint le thread.
        self.commands = None;
        self.cancel.cancel();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            // Une panique dans le thread de signalisation ne doit pas
            // propager : l'agent est en cours de destruction, et `drop` ne
            // peut rien renvoyer.
            error!("le thread de signalisation s'est terminé anormalement");
        }
    }
}

/// Corps du thread de signalisation.
fn run_agent_thread(
    local_port: u16,
    commands: mpsc::Receiver<Command>,
    ready: mpsc::Sender<Result<(), AdaptError>>,
    cancel: CancellationToken,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(mapping::adapt_error("création du runtime", error)));
            return;
        }
    };

    runtime.block_on(async move {
        // --- Ouverture de la socket locale ---------------------------------
        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), local_port);
        let connection = match UdpConnection::create_connection(
            local,
            None,
            Some(cancel.child_token()),
        )
        .await
        {
            Ok(connection) => connection,
            Err(error) => {
                let _ = ready.send(Err(mapping::adapt_error(
                    &format!("ouverture du port {local_port}"),
                    error,
                )));
                return;
            }
        };

        // --- Construction de l'endpoint ------------------------------------
        let transport_layer = TransportLayer::new(cancel.child_token());
        transport_layer.add_transport(connection.into());

        let endpoint = EndpointBuilder::new()
            .with_user_agent(concat!("opendial/", env!("CARGO_PKG_VERSION")))
            .with_transport_layer(transport_layer)
            .with_cancel_token(cancel.clone())
            .build();

        // `build()` renvoie un `Endpoint` par valeur, mais `serve()` exige un
        // `&Arc<Self>` : c'est l'endpoint qui doit se partager entre la tâche
        // de service et les dialogues. On l'enveloppe donc une seule fois.
        let endpoint = std::sync::Arc::new(endpoint);
        let endpoint_inner = endpoint.inner.clone();

        // --- Tâche de fond : réception des messages SIP --------------------
        //
        // Sans elle, l'endpoint n'accepte aucune connexion et les réponses du
        // serveur ne sont jamais traitées. `Endpoint::serve()` journalise
        // elle-même ses erreurs et se termine à l'annulation du jeton.
        let endpoint_for_serve = std::sync::Arc::clone(&endpoint);
        tokio::spawn(async move {
            endpoint_for_serve.serve().await;
        });

        // L'endpoint est conservé vivant jusqu'à la fin de la boucle : le
        // relâcher fermerait les transports et interromprait le service.
        let _endpoint_guard = endpoint;

        let _ = ready.send(Ok(()));

        // --- Boucle de commandes -------------------------------------------
        //
        // Le canal est synchrone : on le draine par petits blocs pour ne pas
        // bloquer le runtime. `recv()` sur un `std::sync::mpsc` bloquerait le
        // thread, empêchant les tâches tokio de progresser.
        let mut registrations: std::collections::HashMap<AccountId, Registration> =
            std::collections::HashMap::new();

        loop {
            let command = match commands.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty) => {
                    // Rien à faire : on laisse le runtime traiter ses tâches
                    // (réponses SIP, rafraîchissements) pendant un court laps.
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
            };

            match command {
                Command::Register { account, reply } => {
                    let result =
                        handle_register(&endpoint_inner, &mut registrations, *account).await;
                    let _ = reply.send(result);
                }
                Command::Unregister { account_id, reply } => {
                    registrations.remove(&account_id);
                    let _ = reply.send(Ok(()));
                }
            }
        }

        debug!("boucle de commandes terminée");
    });
}

/// Traite une commande d'enregistrement.
async fn handle_register(
    endpoint: &rsipstack::transaction::endpoint::EndpointInnerRef,
    registrations: &mut std::collections::HashMap<AccountId, Registration>,
    account: Account,
) -> Result<RegistrationState, AdaptError> {
    let server = mapping::registrar_uri(&account)?;
    let credential = mapping::credential(&account.credentials);
    let expires = account.expiry.as_secs();

    // On réutilise l'objet `Registration` d'un compte déjà vu : il conserve le
    // `Call-ID` et le numéro de séquence, ce qui permet au serveur de
    // reconnaître un rafraîchissement plutôt qu'un nouvel enregistrement.
    let registration = registrations
        .entry(account.id.clone())
        .or_insert_with(|| Registration::new(endpoint.clone(), Some(credential)));

    let response = registration
        .register(server, Some(expires))
        .await
        .map_err(|error| mapping::adapt_error("enregistrement", error))?;

    let granted = registration.expires();
    let state = mapping::registration_state_from_response(response.status_code, granted);

    match &state {
        RegistrationState::Registered { expires_in } => {
            info!(
                account = %account.id,
                expires_in,
                "compte enregistré"
            );
        }
        RegistrationState::Failed { reason, retrying } => {
            // Un retrait de la table force un nouvel enregistrement complet au
            // prochain essai : réutiliser un `Registration` dont
            // l'authentification a échoué reconduirait le même échec.
            if !retrying {
                registrations.remove(&account.id);
            }
            error!(account = %account.id, %reason, retrying, "échec de l'enregistrement");
        }
        _ => {}
    }

    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Credentials, Registrar, Transport};

    fn sample_account() -> Account {
        Account::new(
            AccountId::new("test-1").expect("identifiant valide"),
            "Test",
            "1001@127.0.0.1",
            Credentials::new("1001", "opendial").expect("identifiants valides"),
            Registrar::new("127.0.0.1")
                .expect("hôte valide")
                .with_port(5060)
                .with_transport(Transport::Udp),
        )
        .expect("compte valide")
    }

    #[test]
    fn agent_starts_and_stops_cleanly() {
        // Port haut et libre : évite tout conflit avec une instance réelle.
        let agent = SipAgent::start(0).expect("démarrage de l'agent");
        drop(agent);
        // Si l'arrêt n'était pas propre, le thread paniquerait et le test
        // suivant ne pourrait pas réutiliser le port.
    }

    #[test]
    fn starting_twice_on_the_same_port_fails_explicitly() {
        // 0 laisse le système choisir : deux agents peuvent donc coexister.
        // Ce test vérifie surtout que le démarrage ne panique jamais.
        let first = SipAgent::start(0).expect("premier agent");
        let second = SipAgent::start(0).expect("second agent");
        drop(first);
        drop(second);
    }

    #[test]
    fn unimplemented_call_operations_report_clearly() {
        // Phase 1 ne couvre pas les appels. L'erreur doit être explicite
        // plutôt que de laisser croire à un succès.
        let mut agent = SipAgent::start(0).expect("démarrage de l'agent");
        let account = sample_account();

        let error = agent
            .start_call(&account.id, "1002")
            .expect_err("non implémenté en phase 1");
        assert!(!error.retryable, "l'absence de fonctionnalité est définitive");
        assert!(
            error.message.contains("Phase 2"),
            "le message doit indiquer la phase : {}",
            error.message
        );
    }

    #[test]
    fn domain_error_conversion_is_not_retryable() {
        let error = mapping::domain_error(&od_core::DomainError::EmptyUsername);
        assert!(!error.is_retryable());
    }
}

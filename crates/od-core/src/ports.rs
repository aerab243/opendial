// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ports du domaine — les traits que les adaptateurs doivent implémenter.
//!
//! ## Le principe d'inversion
//!
//! Le domaine **définit** ces traits ; les crates d'adaptation les
//! **implémentent**. Le domaine ne dépend donc jamais d'une bibliothèque
//! externe : c'est la bibliothèque qui dépend de lui.
//!
//! ```text
//!   od-core  ──définit──►  trait SignalingPort  ◄──implémente──  od-sip
//!   od-core  ──définit──►  trait MediaPort      ◄──implémente──  od-media
//! ```
//!
//! ## Pourquoi c'est indispensable ici
//!
//! `rsipstack` et `rustrtc` sont des crates **0.x maintenus par un seul
//! auteur** (voir `docs/adr/0002`). Si l'un d'eux est abandonné, on écrit un
//! nouvel adaptateur derrière ces traits — **le domaine, ses tests et
//! l'interface Flutter restent intacts**. C'est exactement ce que MicroSIP ne
//! pouvait pas faire : soudé à PJSIP via `pjsua_internal.h`, il ne pourra
//! jamais changer de stack (voir `docs/adr/0001`).
//!
//! ## Forme des traits
//!
//! Les traits sont **synchrones** et sans `async`. Ce choix est délibéré :
//!
//! - Le domaine n'embarque aucun runtime asynchrone. `async fn` dans un trait
//!   forcerait `od-core` à dépendre de `tokio` ou d'`async-trait`, ce qui
//!   contredirait sa raison d'être.
//! - Les adaptateurs asynchrones (rsipstack, rustrtc) exposent des API
//!   `async` : ils implémentent ces traits en déléguant à un runtime interne,
//!   et publient leurs résultats par flux d'événements.
//!
//! Les tests du domaine, eux, n'ont besoin que d'implémentations en mémoire —
//! ce que ce découpage rend trivial.

use crate::account::{Account, AccountId};
use crate::call::{Call, CallId};
use crate::registration::RegistrationState;

/// Erreur remontée par un adaptateur de signalisation.
///
/// Volontairement opaque : le domaine n'a pas à connaître les codes de statut
/// SIP ni les erreurs de socket. L'adaptateur traduit vers un message que
/// l'interface peut afficher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalingError {
    /// Message lisible par un humain, destiné à l'interface.
    pub message: String,
    /// L'opération peut-elle être retentée ?
    ///
    /// Pilote la stratégie de reprise : on ne réessaie pas indéfiniment un
    /// enregistrement refusé pour identifiants invalides.
    pub retryable: bool,
}

impl SignalingError {
    /// Crée une erreur transitoire — une reprise est pertinente.
    #[must_use]
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }

    /// Crée une erreur définitive — réessayer ne servirait à rien.
    #[must_use]
    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

impl std::fmt::Display for SignalingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SignalingError {}

/// Port de signalisation : enregistrement et contrôle des appels.
///
/// C'est le contrat que tout stack SIP doit satisfaire pour être utilisable par
/// opendial. L'implémentation de référence est `od-sip` (rsipstack), mais le
/// domaine n'en sait rien.
///
/// # Pourquoi `Send`
///
/// La contrainte `Send` vient de l'interface : Flutter doit pouvoir détenir un
/// service de longue durée dans un état partagé, accessible depuis plusieurs
/// fils d'exécution. Un adaptateur non `Send` forcerait la FFI à sérialiser
/// tous les appels sur un seul thread, ce qui nuirait à la réactivité.
///
/// Elle n'a aucun coût pour les implémentations en mémoire : la plupart des
/// types le sont déjà naturellement.
pub trait SignalingPort: Send {
    /// Enregistre un compte auprès de son registrar (RFC 3261 §10).
    ///
    /// # Erreurs
    ///
    /// Renvoie [`SignalingError`] si le serveur refuse ou est injoignable.
    fn register(&mut self, account: &Account) -> Result<RegistrationState, SignalingError>;

    /// Désenregistre un compte en demandant une expiration nulle.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`SignalingError`] si la désinscription échoue.
    fn unregister(&mut self, account_id: &AccountId) -> Result<(), SignalingError>;

    /// Établit un appel sortant vers `destination`.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`SignalingError`] si la requête ne peut pas être émise. Un
    /// refus du correspondant n'est **pas** une erreur à ce niveau : il est
    /// signalé par un événement [`crate::call::CallEvent`] terminal.
    fn start_call(
        &mut self,
        account_id: &AccountId,
        destination: &str,
    ) -> Result<Call, SignalingError>;

    /// Accepte un appel entrant.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`SignalingError`] si l'appel n'existe plus ou n'est pas en
    /// attente de décision.
    fn accept_call(&mut self, call_id: &CallId) -> Result<(), SignalingError>;

    /// Refuse un appel entrant.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`SignalingError`] si l'appel n'existe plus.
    fn reject_call(&mut self, call_id: &CallId) -> Result<(), SignalingError>;

    /// Termine un appel en cours.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`SignalingError`] si l'appel n'existe plus.
    fn hangup_call(&mut self, call_id: &CallId) -> Result<(), SignalingError>;
}

/// Port média : ouverture, fermeture et réglage des flux audio.
///
/// Distinct du port de signalisation : c'est le principe même du découpage.
/// L'audio entrant dans un appel peut se régler sans parler au serveur, et la
/// négociation SDP se conduit sans toucher à la carte son.
///
/// La contrainte `Send` suit la même raison que pour [`SignalingPort`] : le
/// service doit pouvoir être partagé entre les fils d'exécution de la FFI.
pub trait MediaPort: Send {
    /// Ouvre une session média à partir d'un flux SDP négocié.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`MediaError`] si aucun codec commun n'existe ou si les ports
    /// locaux ne peuvent pas être ouverts.
    fn open(&mut self, call_id: &CallId, sdp: &str) -> Result<MediaSessionInfo, MediaError>;

    /// Ferme la session média d'un appel.
    ///
    /// Sans effet si l'appel n'a pas de session ouverte.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`MediaError`] si la fermeture échoue anormalement.
    fn close(&mut self, call_id: &CallId) -> Result<(), MediaError>;

    /// Coupe l'entrée micro sans interrompre le flux.
    ///
    /// Le flux continue d'être émis (silence) : interrompre le RTP ferait
    /// croire à une coupure réseau et déclencherait une renégociation.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`MediaError`] si l'appel n'a pas de session active.
    fn set_mute(&mut self, call_id: &CallId, muted: bool) -> Result<(), MediaError>;

    /// Coupe la sortie haut-parleur.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`MediaError`] si l'appel n'a pas de session active.
    fn set_speaker_mute(&mut self, call_id: &CallId, muted: bool) -> Result<(), MediaError>;

    /// Suspend le flux média, typiquement pour une mise en attente.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`MediaError`] si l'appel n'a pas de session active.
    fn set_hold(&mut self, call_id: &CallId, held: bool) -> Result<(), MediaError>;
}

/// Erreur remontée par un adaptateur média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaError {
    /// Message lisible par un humain.
    pub message: String,
}

impl MediaError {
    /// Crée une erreur média.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MediaError {}

/// Caractéristiques d'une session média ouverte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSessionInfo {
    /// Codec négocié et retenu.
    ///
    /// Ex. `"PCMU"` pour G.711 µ-law, `"opus"` pour Opus.
    pub codec: String,
    /// Port RTP local effectivement utilisé.
    pub local_port: u16,
    /// Adresse distante vers laquelle émettre le RTP.
    pub remote_addr: String,
    /// Période d'émission (packetisation) en millisecondes.
    pub ptime_ms: u16,
}

impl MediaSessionInfo {
    /// Crée une description de session média.
    #[must_use]
    pub fn new(
        codec: impl Into<String>,
        local_port: u16,
        remote_addr: impl Into<String>,
        ptime_ms: u16,
    ) -> Self {
        Self {
            codec: codec.into(),
            local_port,
            remote_addr: remote_addr.into(),
            ptime_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountId, Credentials, Registrar};

    /// Adaptateur de test : implémente le port sans aucun réseau.
    ///
    /// C'est la démonstration concrète de l'intérêt des ports — un scénario
    /// complet se teste en mémoire, sans serveur SIP ni carte son.
    #[derive(Default)]
    struct FakeSignaling {
        registered: Option<AccountId>,
        calls_started: Vec<String>,
    }

    impl SignalingPort for FakeSignaling {
        fn register(&mut self, account: &Account) -> Result<RegistrationState, SignalingError> {
            self.registered = Some(account.id.clone());
            Ok(RegistrationState::Registered { expires_in: 300 })
        }

        fn unregister(&mut self, account_id: &AccountId) -> Result<(), SignalingError> {
            if self.registered.as_ref() == Some(account_id) {
                self.registered = None;
            }
            Ok(())
        }

        fn start_call(
            &mut self,
            _account_id: &AccountId,
            destination: &str,
        ) -> Result<Call, SignalingError> {
            self.calls_started.push(destination.to_owned());
            Ok(Call::outgoing(CallId::new("fake-1"), destination))
        }

        fn accept_call(&mut self, _call_id: &CallId) -> Result<(), SignalingError> {
            Ok(())
        }

        fn reject_call(&mut self, _call_id: &CallId) -> Result<(), SignalingError> {
            Ok(())
        }

        fn hangup_call(&mut self, _call_id: &CallId) -> Result<(), SignalingError> {
            Ok(())
        }
    }

    fn sample_account() -> Account {
        Account::new(
            AccountId::new("a1").expect("identifiant valide"),
            "Test",
            "1001@pbx.example.com",
            Credentials::new("1001", "secret").expect("identifiants valides"),
            Registrar::new("pbx.example.com").expect("hôte valide"),
        )
        .expect("compte valide")
    }

    #[test]
    fn domain_is_testable_without_any_io() {
        // Aucune socket, aucun serveur, aucune carte son : le scénario complet
        // s'exécute en mémoire. C'est précisément ce que MicroSIP ne permettait
        // pas de faire.
        let mut signaling = FakeSignaling::default();
        let account = sample_account();

        let state = signaling.register(&account).expect("enregistrement");
        assert!(state.is_registered());
        assert_eq!(signaling.registered, Some(account.id.clone()));

        let call = signaling
            .start_call(&account.id, "1002")
            .expect("appel sortant");
        assert_eq!(call.remote(), "1002");
        assert_eq!(signaling.calls_started, vec!["1002"]);

        signaling.unregister(&account.id).expect("désinscription");
        assert!(signaling.registered.is_none());
    }

    #[test]
    fn signaling_error_distinguishes_retryability() {
        let transient = SignalingError::retryable("serveur injoignable");
        assert!(transient.retryable);

        let permanent = SignalingError::permanent("identifiants invalides");
        assert!(!permanent.retryable);

        assert!(std::error::Error::source(&transient).is_none());
        assert_eq!(permanent.to_string(), "identifiants invalides");
    }

    #[test]
    fn media_session_info_describes_negotiated_stream() {
        let info = MediaSessionInfo::new("PCMU", 5004, "192.0.2.10:5006", 20);
        assert_eq!(info.codec, "PCMU");
        assert_eq!(info.local_port, 5004);
        assert_eq!(info.remote_addr, "192.0.2.10:5006");
        assert_eq!(info.ptime_ms, 20);
    }
}

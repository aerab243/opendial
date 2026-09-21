// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Service de comptes : orchestration entre le domaine, l'adaptateur SIP et
//! l'interface.
//!
//! ## Ce que ce module fait — et ne fait pas
//!
//! Il **orchestre** : il retient quel compte est enregistré, appelle
//! l'adaptateur, et publie les changements d'état. Il ne **décide** rien :
//! toute règle métier vit dans `od-core`, et toute connaissance du protocole
//! dans `od-sip`.
//!
//! Cette séparation est ce qui permet de tester le service sans serveur SIP
//! (via un adaptateur factice) et de tester le domaine sans interface.

use od_config::{AccountStore, ConfigError};
use od_core::{
    Account, AccountId, Credentials, Registrar, RegistrationState, SignalingPort, Transport,
};
use tracing::{debug, info, warn};

use crate::error::FfiError;

/// Description d'un compte, telle que l'interface la reçoit.
///
/// Type de transfert volontairement plat : la FFI ne peut pas exposer les
/// types du domaine, dont l'évolution est indépendante du contrat avec Dart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    /// Identifiant stable.
    pub id: String,
    /// Libellé affiché.
    pub label: String,
    /// Adresse d'enregistrement.
    pub address_of_record: String,
    /// Hôte du serveur.
    pub registrar_host: String,
    /// Port, ou 0 si le défaut du transport s'applique.
    pub registrar_port: u16,
    /// Transport, sous forme de chaîne stable (`"udp"`, `"tcp"`, `"tls"`).
    pub transport: String,
    /// État d'enregistrement, sous forme de chaîne stable.
    pub status: String,
    /// Détail lisible, vide si rien à signaler.
    pub status_detail: String,
}

/// Paramètres de création d'un compte, fournis par l'interface.
///
/// Distinct de [`Account`] : c'est un type de **transfert**, sans invariants.
/// Le domaine validera ces valeurs à la construction ; les confondre
/// permettrait à l'interface de fabriquer un compte invalide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDraft {
    /// Libellé affiché.
    pub label: String,
    /// Nom d'utilisateur SIP.
    pub username: String,
    /// Mot de passe.
    pub password: String,
    /// Adresse du serveur (hôte, éventuellement suivie de `:port`).
    pub server: String,
    /// Transport : `"udp"`, `"tcp"` ou `"tls"`.
    pub transport: String,
}

/// Service gérant le cycle de vie des comptes.
pub struct AccountService {
    store: AccountStore,
    signaling: Box<dyn SignalingPort>,
    states: std::collections::HashMap<AccountId, RegistrationState>,
}

impl AccountService {
    /// Crée un service adossé à un adaptateur de signalisation.
    ///
    /// L'adaptateur est injecté plutôt que construit ici : les tests peuvent
    /// ainsi fournir une implémentation en mémoire, et le service reste
    /// testable sans réseau.
    #[must_use]
    pub fn new(signaling: Box<dyn SignalingPort>) -> Self {
        Self {
            store: AccountStore::new(),
            signaling,
            states: std::collections::HashMap::new(),
        }
    }

    /// Ajoute un compte et tente de l'enregistrer.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`FfiError`] si les paramètres sont invalides, si l'identifiant
    /// est déjà pris, ou si l'enregistrement échoue. Un échec
    /// d'enregistrement n'annule pas l'ajout : le compte reste configuré et
    /// pourra être réessayé.
    pub fn add_account(&mut self, draft: AccountDraft) -> Result<AccountInfo, FfiError> {
        let account = build_account(draft)?;
        let id = account.id.clone();

        self.store.add(account.clone()).map_err(FfiError::from)?;
        info!(account = %id, "compte ajouté");

        // L'échec d'enregistrement est consigné dans l'état du compte plutôt
        // que remonté comme une erreur : l'utilisateur a bien créé son compte,
        // et l'interface doit pouvoir lui montrer pourquoi le serveur refuse.
        let state = match self.signaling.register(&account) {
            Ok(state) => state,
            Err(error) => {
                warn!(account = %id, error = %error, "enregistrement initial refusé");
                if error.retryable {
                    RegistrationState::failed_retrying(error.message)
                } else {
                    RegistrationState::failed_permanent(error.message)
                }
            }
        };
        self.states.insert(id, state);

        self.info_for(&account.id)
            .ok_or_else(|| FfiError::new("compte introuvable juste après son ajout"))
    }

    /// Relance l'enregistrement d'un compte existant.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`FfiError`] si le compte est inconnu.
    pub fn register_account(&mut self, account_id: &str) -> Result<AccountInfo, FfiError> {
        let id = AccountId::new(account_id).map_err(FfiError::from)?;

        let account = self
            .store
            .get(&id)
            .cloned()
            .ok_or_else(|| FfiError::new(format!("aucun compte « {account_id} »")))?;

        let state = match self.signaling.register(&account) {
            Ok(state) => state,
            Err(error) => {
                if error.retryable {
                    RegistrationState::failed_retrying(error.message)
                } else {
                    RegistrationState::failed_permanent(error.message)
                }
            }
        };
        self.states.insert(id.clone(), state);

        self.info_for(&id)
            .ok_or_else(|| FfiError::new("compte introuvable"))
    }

    /// Désenregistre un compte et le retire.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`FfiError`] si le compte est inconnu.
    pub fn remove_account(&mut self, account_id: &str) -> Result<(), FfiError> {
        let id = AccountId::new(account_id).map_err(FfiError::from)?;

        // Une désinscription qui échoue ne doit pas empêcher le retrait : le
        // serveur oubliera le contact à l'expiration, et l'utilisateur a
        // demandé la suppression.
        if let Err(error) = self.signaling.unregister(&id) {
            debug!(account = %id, error = %error, "désinscription refusée, le compte est tout de même retiré");
        }

        self.store.remove(&id).map_err(FfiError::from)?;
        self.states.remove(&id);
        info!(account = %id, "compte retiré");
        Ok(())
    }

    /// Renvoie la liste des comptes avec leur état.
    #[must_use]
    pub fn list_accounts(&self) -> Vec<AccountInfo> {
        self.store
            .all()
            .iter()
            .filter_map(|account| self.info_for(&account.id))
            .collect()
    }

    /// Renvoie l'état d'un compte, ou `None` s'il est inconnu.
    #[must_use]
    pub fn info_for(&self, id: &AccountId) -> Option<AccountInfo> {
        let account = self.store.get(id)?;
        let state = self.states.get(id);

        let (status, status_detail) = match state {
            Some(RegistrationState::Registered { expires_in }) => {
                ("registered".to_owned(), format!("{expires_in} s"))
            }
            Some(RegistrationState::Registering) => ("registering".to_owned(), String::new()),
            Some(RegistrationState::Failed { reason, retrying }) => {
                let detail = if *retrying {
                    format!("{reason} (nouvelle tentative planifiée)")
                } else {
                    reason.clone()
                };
                ("failed".to_owned(), detail)
            }
            // Aucun état connu : le compte vient d'être créé et n'a jamais été
            // enregistré. À distinguer d'un échec.
            Some(RegistrationState::Unregistered) | None => {
                ("unregistered".to_owned(), String::new())
            }
        };

        Some(AccountInfo {
            id: account.id.to_string(),
            label: account.label.clone(),
            address_of_record: account.address_of_record.clone(),
            registrar_host: account.registrar.host.clone(),
            registrar_port: account.registrar.port.unwrap_or(0),
            transport: account.registrar.transport.as_str().to_owned(),
            status,
            status_detail,
        })
    }
}

/// Construit un compte du domaine à partir d'un brouillon d'interface.
///
/// C'est ici que les saisies utilisateur rencontrent les invariants du
/// domaine : les erreurs remontent en messages lisibles, prêts à être
/// affichés.
fn build_account(draft: AccountDraft) -> Result<Account, FfiError> {
    let (host, port) = split_server(&draft.server);
    if host.is_empty() {
        return Err(FfiError::new("l'adresse du serveur est vide"));
    }

    let transport = parse_transport(&draft.transport)?;

    // `host` sert aussi à construire l'adresse d'enregistrement plus bas :
    // on le clone plutôt que de le déplacer.
    let mut registrar = Registrar::new(host.clone()).map_err(FfiError::from)?;
    if let Some(port) = port {
        registrar = registrar.with_port(port);
    }
    registrar = registrar.with_transport(transport);

    let credentials =
        Credentials::new(draft.username.clone(), draft.password).map_err(FfiError::from)?;

    // L'adresse d'enregistrement reprend l'utilisateur et l'hôte du serveur :
    // c'est la convention pour un poste SIP, et cela évite d'imposer à
    // l'utilisateur de saisir deux fois la même information.
    let address_of_record = format!("{}@{host}", draft.username);

    Account::new(
        AccountId::new(draft.username.clone()).map_err(FfiError::from)?,
        draft.label,
        address_of_record,
        credentials,
        registrar,
    )
    .map_err(FfiError::from)
}

/// Sépare `hôte` et `hôte:port`.
fn split_server(server: &str) -> (String, Option<u16>) {
    let trimmed = server.trim();
    match trimmed.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => (host.trim().to_owned(), Some(port)),
            // Un suffixe non numérique n'est pas un port : on considère toute
            // la chaîne comme un hôte, ce qui couvre les noms contenant un
            // deux-points sans être un port.
            Err(_) => (trimmed.to_owned(), None),
        },
        None => (trimmed.to_owned(), None),
    }
}

/// Convertit une chaîne de transport en valeur du domaine.
fn parse_transport(transport: &str) -> Result<Transport, FfiError> {
    match transport.trim().to_ascii_lowercase().as_str() {
        "udp" | "" => Ok(Transport::Udp),
        "tcp" => Ok(Transport::Tcp),
        "tls" => Ok(Transport::Tls),
        other => Err(FfiError::new(format!(
            "transport inconnu « {other} » — attendu udp, tcp ou tls"
        ))),
    }
}

impl From<ConfigError> for FfiError {
    fn from(value: ConfigError) -> Self {
        Self::new(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Call, CallId, SignalingError};

    /// Adaptateur factice : enregistre en mémoire, sans réseau.
    ///
    /// C'est la démonstration concrète de l'intérêt des traits-ports — tout le
    /// service se teste sans serveur SIP.
    #[derive(Default)]
    struct FakeSignaling {
        fail_with: Option<SignalingError>,
        registered: Vec<AccountId>,
        unregistered: Vec<AccountId>,
    }

    impl FakeSignaling {
        fn failing(error: SignalingError) -> Self {
            Self {
                fail_with: Some(error),
                ..Self::default()
            }
        }
    }

    impl SignalingPort for FakeSignaling {
        fn register(&mut self, account: &Account) -> Result<RegistrationState, SignalingError> {
            if let Some(error) = &self.fail_with {
                return Err(error.clone());
            }
            self.registered.push(account.id.clone());
            Ok(RegistrationState::Registered { expires_in: 300 })
        }

        fn unregister(&mut self, account_id: &AccountId) -> Result<(), SignalingError> {
            self.unregistered.push(account_id.clone());
            Ok(())
        }

        fn start_call(
            &mut self,
            _account_id: &AccountId,
            _destination: &str,
        ) -> Result<Call, SignalingError> {
            Ok(Call::outgoing(CallId::new("fake"), "1002"))
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

    fn draft() -> AccountDraft {
        AccountDraft {
            label: "Travail".to_owned(),
            username: "1001".to_owned(),
            password: "opendial".to_owned(),
            server: "127.0.0.1:5060".to_owned(),
            transport: "udp".to_owned(),
        }
    }

    #[test]
    fn adding_an_account_registers_it_immediately() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let info = service.add_account(draft()).expect("ajout du compte");

        assert_eq!(info.id, "1001");
        assert_eq!(info.label, "Travail");
        // L'ajout déclenche l'enregistrement : l'utilisateur n'a pas à faire
        // deux actions pour configurer un compte.
        assert_eq!(info.status, "registered");
        assert_eq!(info.status_detail, "300 s");
    }

    #[test]
    fn server_string_is_split_into_host_and_port() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let info = service.add_account(draft()).expect("ajout");
        assert_eq!(info.registrar_host, "127.0.0.1");
        assert_eq!(info.registrar_port, 5060);
    }

    #[test]
    fn server_without_port_reports_zero() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let mut d = draft();
        d.server = "pbx.example.com".to_owned();
        let info = service.add_account(d).expect("ajout");
        assert_eq!(info.registrar_host, "pbx.example.com");
        // 0 signale que le port par défaut du transport s'applique.
        assert_eq!(info.registrar_port, 0);
    }

    #[test]
    fn registration_failure_keeps_the_account_configured() {
        // Un serveur injoignable ne doit pas faire perdre la saisie : le
        // compte reste configuré, prêt à être réessayé.
        let signaling = FakeSignaling::failing(SignalingError::retryable("serveur injoignable"));
        let mut service = AccountService::new(Box::new(signaling));

        let info = service.add_account(draft()).expect("le compte est créé");
        assert_eq!(info.status, "failed");
        assert!(info.status_detail.contains("serveur injoignable"));
        assert_eq!(service.list_accounts().len(), 1);
    }

    #[test]
    fn permanent_failure_is_reported_without_retry_hint() {
        let signaling = FakeSignaling::failing(SignalingError::permanent("identifiants invalides"));
        let mut service = AccountService::new(Box::new(signaling));

        let info = service.add_account(draft()).expect("le compte est créé");
        assert_eq!(info.status, "failed");
        assert!(
            !info.status_detail.contains("nouvelle tentative"),
            "un échec définitif ne doit pas annoncer de reprise : {}",
            info.status_detail
        );
    }

    #[test]
    fn re_registering_an_existing_account_works() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        service.add_account(draft()).expect("ajout");

        let info = service.register_account("1001").expect("nouvel essai");
        assert_eq!(info.status, "registered");
    }

    #[test]
    fn re_registering_an_unknown_account_fails_clearly() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let error = service
            .register_account("inexistant")
            .expect_err("compte inconnu");
        assert!(error.message.contains("inexistant"));
    }

    #[test]
    fn removing_an_account_unregisters_it() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        service.add_account(draft()).expect("ajout");

        service.remove_account("1001").expect("retrait");
        assert!(service.list_accounts().is_empty());
        assert!(
            service
                .info_for(&AccountId::new("1001").expect("id"))
                .is_none()
        );
    }

    #[test]
    fn accounts_are_listed_sorted_by_label() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));

        let mut z = draft();
        z.label = "Zèbre".to_owned();
        z.username = "1009".to_owned();
        service.add_account(z).expect("ajout");

        let mut a = draft();
        a.label = "Alpha".to_owned();
        a.username = "1002".to_owned();
        service.add_account(a).expect("ajout");

        // Le vecteur est lié à une variable : le collecter directement depuis
        // un temporaire ferait tomber la référence avant la comparaison.
        let accounts = service.list_accounts();
        let labels: Vec<&str> = accounts.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["Alpha", "Zèbre"]);
    }

    // --- Validation des saisies ------------------------------------------

    #[test]
    fn empty_server_is_refused() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let mut d = draft();
        d.server = "   ".to_owned();
        let error = service.add_account(d).expect_err("serveur vide");
        assert!(error.message.contains("serveur"));
    }

    #[test]
    fn unknown_transport_is_refused() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let mut d = draft();
        d.transport = "carrier-pigeon".to_owned();
        let error = service.add_account(d).expect_err("transport invalide");
        assert!(error.message.contains("udp"));
    }

    #[test]
    fn empty_username_is_refused() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let mut d = draft();
        d.username = String::new();
        assert!(service.add_account(d).is_err());
    }

    #[test]
    fn transport_names_are_case_insensitive() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        let mut d = draft();
        d.transport = "TLS".to_owned();
        let info = service.add_account(d).expect("ajout");
        assert_eq!(info.transport, "tls");
    }

    #[test]
    fn duplicate_username_is_refused() {
        let mut service = AccountService::new(Box::new(FakeSignaling::default()));
        service.add_account(draft()).expect("premier ajout");
        assert!(service.add_account(draft()).is_err(), "doublon refusé");
    }
}

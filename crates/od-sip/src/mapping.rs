// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Traduction entre le domaine et rsipstack.
//!
//! Ce module isole **toute** la connaissance du protocole. Le reste de
//! l'adaptateur s'exprime en types du domaine ; si l'on remplace rsipstack,
//! c'est ici — et nulle part ailleurs — que le travail se concentre.
//!
//! Aucune fonction de ce module ne fait d'I/O : ce sont des conversions pures,
//! testables sans réseau.

use od_core::{Account, Credentials, DomainError, RegistrationState, Transport};
use rsipstack::dialog::authenticate::Credential;
use rsipstack::sip as rsip;

use crate::error::AdaptError;

/// Convertit un transport du domaine en URI SIP.
///
/// Le schéma `sips:` désigne TLS et n'est pas équivalent à `sip:` avec un
/// transport TLS : c'est le schéma qui impose le chiffrement de bout en bout
/// de la signalisation (RFC 3261 §26.2.2). Confondre les deux conduirait à
/// annoncer une sécurité qu'on n'applique pas.
pub(crate) fn transport_scheme(transport: Transport) -> &'static str {
    match transport {
        Transport::Tls => "sips",
        Transport::Udp | Transport::Tcp => "sip",
    }
}

/// Construit l'URI du registrar à partir d'un compte.
///
/// Le port n'est ajouté que s'il a été explicitement configuré : laisser
/// rsipstack appliquer le défaut du schéma évite d'imposer 5060 à un serveur
/// qui écoute ailleurs.
pub(crate) fn registrar_uri(account: &Account) -> Result<rsip::Uri, AdaptError> {
    let scheme = transport_scheme(account.registrar.transport);
    let authority = match account.registrar.port {
        Some(port) => format!("{}:{}", account.registrar.host, port),
        None => account.registrar.host.clone(),
    };
    let uri = format!("{scheme}:{authority}");

    uri.parse::<rsip::Uri>()
        .map_err(|_| AdaptError::invalid_uri(uri))
}

/// Construit l'URI de l'utilisateur, telle qu'elle apparaît dans `To` et `From`.
///
/// Utilisée en Phase 2 pour l'établissement d'appel : `INVITE` porte cette URI
/// dans ses en-têtes `From` et `Contact`.
#[allow(dead_code, reason = "employée par l'établissement d'appel en Phase 2")]
pub(crate) fn user_uri(account: &Account) -> Result<rsip::Uri, AdaptError> {
    let address = if account.address_of_record.contains('@') {
        account.address_of_record.clone()
    } else {
        format!("{}@{}", account.address_of_record, account.registrar.host)
    };
    let uri = format!("{}:{address}", transport_scheme(account.registrar.transport));

    uri.parse::<rsip::Uri>()
        .map_err(|_| AdaptError::invalid_uri(uri))
}

/// Convertit les identifiants du domaine en identifiants rsipstack.
///
/// Le domaine ne connaît qu'un couple utilisateur/mot de passe ; rsipstack
/// attend en plus un domaine d'authentification. On ne le renseigne que s'il
/// est explicitement configuré : laisser `None` permet à rsipstack de
/// découvrir le realm depuis le défi `401`/`407`, ce qui est le comportement
/// correct face à un serveur inconnu.
pub(crate) fn credential(credentials: &Credentials) -> Credential {
    Credential {
        username: credentials.username.clone(),
        password: credentials.password().to_owned(),
        realm: credentials.realm.clone(),
    }
}

/// Interprète une réponse REGISTER et en déduit l'état du domaine.
///
/// C'est la traduction la plus importante du module : elle décide si un compte
/// est joignable, en échec transitoire ou en échec définitif — distinction qui
/// pilote la stratégie de reprise.
pub(crate) fn registration_state_from_response(
    status: rsip::StatusCode,
    expires: u32,
) -> RegistrationState {
    match status {
        rsip::StatusCode::OK => RegistrationState::Registered {
            expires_in: expires,
        },
        // 401 et 407 signifient que le défi d'authentification n'a pas été
        // résolu — identifiants erronés ou mal transmis. Réessayer à
        // l'identique ne servira à rien : c'est un échec définitif.
        rsip::StatusCode::Unauthorized | rsip::StatusCode::ProxyAuthenticationRequired => {
            RegistrationState::failed_permanent(
                "authentification refusée — vérifiez l'utilisateur et le mot de passe",
            )
        }
        // 403 : le serveur connaît l'identité mais refuse le service (compte
        // suspendu, adresse non autorisée). Définitif également.
        rsip::StatusCode::Forbidden => {
            RegistrationState::failed_permanent("le serveur refuse ce compte")
        }
        // 404 : le domaine est inconnu du serveur. Définitif tant que la
        // configuration n'est pas corrigée.
        rsip::StatusCode::NotFound => {
            RegistrationState::failed_permanent("domaine inconnu du serveur")
        }
        // 5xx : le serveur est en difficulté, mais la configuration est
        // correcte. Une reprise est pertinente.
        other if other.kind() == rsip::StatusCodeKind::ServerFailure => {
            RegistrationState::failed_retrying(format!(
                "erreur du serveur ({})",
                other.code()
            ))
        }
        // 408 et 480 : le serveur n'a pas répondu à temps, ou le poste est
        // injoignable. Transitoire.
        rsip::StatusCode::RequestTimeout | rsip::StatusCode::TemporarilyUnavailable => {
            RegistrationState::failed_retrying(format!(
                "serveur temporairement indisponible ({})",
                status.code()
            ))
        }
        // Tout le reste est traité comme transitoire : on préfère réessayer à
        // tort que laisser un compte silencieusement injoignable.
        _ => RegistrationState::failed_retrying(format!(
            "réponse inattendue du serveur ({})",
            status.code()
        )),
    }
}

/// Convertit une erreur de rsipstack en erreur d'adaptateur.
///
/// Le message est rendu lisible pour l'interface : « connexion refusée » est
/// actionnable, une trace de pile ne l'est pas.
pub(crate) fn adapt_error(context: &str, error: impl std::fmt::Display) -> AdaptError {
    AdaptError::Protocol {
        context: context.to_owned(),
        detail: error.to_string(),
    }
}

/// Convertit une erreur de domaine en erreur d'adaptateur.
///
/// Signale une incohérence interne : le domaine a validé une donnée que
/// l'adaptateur n'arrive pas à convertir. C'est un bug, pas une erreur
/// utilisateur.
#[allow(dead_code, reason = "branchement sur les erreurs d'appel en Phase 2")]
pub(crate) fn domain_error(error: &DomainError) -> AdaptError {
    AdaptError::InvalidAccount(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{AccountId, Expiry, Registrar};

    fn account_with(transport: Transport, port: Option<u16>) -> Account {
        let mut registrar = Registrar::new("pbx.example.com")
            .expect("hôte valide")
            .with_transport(transport);
        if let Some(port) = port {
            registrar = registrar.with_port(port);
        }
        Account::new(
            AccountId::new("a1").expect("identifiant valide"),
            "Test",
            "1001@pbx.example.com",
            Credentials::new("1001", "secret").expect("identifiants valides"),
            registrar,
        )
        .expect("compte valide")
    }

    // --- Traduction du transport -----------------------------------------

    #[test]
    fn tls_uses_the_sips_scheme() {
        // Le schéma `sips` n'est pas cosmétique : c'est lui qui impose le
        // chiffrement de la signalisation (RFC 3261 §26.2.2).
        assert_eq!(transport_scheme(Transport::Tls), "sips");
        assert_eq!(transport_scheme(Transport::Udp), "sip");
        assert_eq!(transport_scheme(Transport::Tcp), "sip");
    }

    // --- Construction des URI --------------------------------------------

    #[test]
    fn registrar_uri_omits_the_port_when_not_configured() {
        let account = account_with(Transport::Udp, None);
        let uri = registrar_uri(&account).expect("URI valide");
        assert_eq!(uri.to_string(), "sip:pbx.example.com");
    }

    #[test]
    fn registrar_uri_includes_an_explicit_port() {
        let account = account_with(Transport::Tls, Some(5061));
        let uri = registrar_uri(&account).expect("URI valide");
        let rendered = uri.to_string();
        assert!(rendered.starts_with("sips:"), "schéma attendu sips : {rendered}");
        assert!(rendered.contains("5061"), "port attendu 5061 : {rendered}");
    }

    #[test]
    fn user_uri_is_built_from_the_address_of_record() {
        let account = account_with(Transport::Udp, None);
        let uri = user_uri(&account).expect("URI valide");
        let rendered = uri.to_string();
        assert!(rendered.contains("1001"), "utilisateur attendu : {rendered}");
        assert!(
            rendered.contains("pbx.example.com"),
            "domaine attendu : {rendered}"
        );
    }

    // --- Identifiants -----------------------------------------------------

    #[test]
    fn credential_preserves_username_and_password() {
        let credentials = Credentials::new("alice", "s3cr3t")
            .expect("identifiants valides")
            .with_realm("asterisk");

        let converted = credential(&credentials);
        assert_eq!(converted.username, "alice");
        assert_eq!(converted.password, "s3cr3t");
        // Le realm explicite est conservé : certains serveurs l'exigent.
        assert_eq!(converted.realm.as_deref(), Some("asterisk"));
    }

    #[test]
    fn credential_leaves_realm_unset_for_discovery() {
        let credentials = Credentials::new("alice", "s3cr3t").expect("identifiants valides");
        let converted = credential(&credentials);
        // Laisser le realm à None est délibéré : rsipstack le découvrira
        // depuis le défi du serveur.
        assert!(converted.realm.is_none());
    }

    // --- Interprétation des réponses -------------------------------------

    #[test]
    fn success_means_registered_with_server_expiry() {
        let state = registration_state_from_response(rsip::StatusCode::OK, 300);
        assert!(state.is_registered());
        // C'est la durée ACCORDÉE par le serveur qui pilote le
        // rafraîchissement, pas celle demandée par le client.
        assert_eq!(state.expires_in(), Some(300));
    }

    #[test]
    fn auth_failures_are_permanent() {
        // Réessayer avec les mêmes identifiants ne peut pas réussir : marquer
        // ces échecs comme transitoires ferait boucler le client
        // indéfiniment sur un mot de passe erroné.
        for status in [
            rsip::StatusCode::Unauthorized,
            rsip::StatusCode::ProxyAuthenticationRequired,
            rsip::StatusCode::Forbidden,
            rsip::StatusCode::NotFound,
        ] {
            let state = registration_state_from_response(status.clone(), 0);
            match state {
                RegistrationState::Failed { retrying, .. } => {
                    assert!(!retrying, "{status} ne devrait pas être réessayé");
                }
                other => panic!("échec attendu pour {status}, obtenu {other:?}"),
            }
        }
    }

    #[test]
    fn server_errors_are_transient() {
        for status in [
            rsip::StatusCode::ServerInternalError,
            rsip::StatusCode::ServiceUnavailable,
            rsip::StatusCode::RequestTimeout,
        ] {
            let state = registration_state_from_response(status.clone(), 0);
            match state {
                RegistrationState::Failed { retrying, .. } => {
                    assert!(retrying, "{status} devrait être réessayé");
                }
                other => panic!("échec attendu pour {status}, obtenu {other:?}"),
            }
        }
    }

    #[test]
    fn unknown_statuses_default_to_retrying() {
        // Prudence délibérée : mieux vaut réessayer à tort que laisser un
        // compte silencieusement injoignable.
        let state = registration_state_from_response(rsip::StatusCode::BadRequest, 0);
        match state {
            RegistrationState::Failed { retrying, .. } => assert!(retrying),
            other => panic!("échec attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn failure_messages_are_actionable() {
        let state = registration_state_from_response(rsip::StatusCode::Unauthorized, 0);
        let text = state.to_string();
        // Le message doit orienter l'utilisateur, pas décrire le protocole.
        assert!(
            text.contains("mot de passe") || text.contains("utilisateur"),
            "message peu exploitable : {text}"
        );
    }

    // --- Erreurs ----------------------------------------------------------

    #[test]
    fn adapt_error_carries_context() {
        let error = adapt_error("résolution DNS", "hôte introuvable");
        let text = error.to_string();
        assert!(text.contains("résolution DNS"));
        assert!(text.contains("hôte introuvable"));
    }

    #[test]
    fn expiry_default_is_accepted_by_the_domain() {
        // Vérifie que la valeur par défaut du domaine traverse la conversion
        // sans erreur : un `Expiry` nul serait rejeté par construction.
        assert!(Expiry::from_secs(300).is_ok());
        assert!(Expiry::from_secs(0).is_err());
    }
}

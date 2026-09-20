// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! # od-ffi — façade entre Flutter et le domaine
//!
//! Ce crate est la **seule** surface que Flutter peut atteindre. Il expose des
//! types sérialisables et des fonctions simples, et traduit les appels vers
//! `od-core` et ses adaptateurs.
//!
//! ## Règle absolue : aucune logique ici
//!
//! `od-ffi` ne prend **aucune décision métier**. Elle ne valide pas, ne
//! calcule pas, ne décide pas d'un état. Elle traduit :
//!
//! ```text
//!   Dart  ──appel──►  od-ffi  ──traduit──►  od-core / adaptateurs
//!   Dart  ◄──stream── od-ffi  ◄──événement── od-core / adaptateurs
//! ```
//!
//! Toute règle métier qui apparaîtrait dans ce crate serait un bug
//! d'architecture : elle échapperait aux tests du domaine, et deviendrait
//! invérifiable sans lancer Flutter. C'est exactement le travers qui rend
//! MicroSIP intestable (voir `docs/adr/0001`).
//!
//! ## Pourquoi cette frontière est étroite
//!
//! Chaque fonction exposée ici est un engagement à long terme : une fois
//! utilisée par Dart, la modifier casse l'application. En gardant la surface
//! minimale — et en laissant le domaine riche — on limite le coût des
//! évolutions futures.
//!
//! ## État actuel
//!
//! Phase 0 : le squelette. Les bindings `flutter_rust_bridge` sont générés à
//! l'étape 5 de la Phase 0, après validation du pont par un projet témoin.

use od_core::{AccountId, CallId, CallState, RegistrationState};

/// Erreur franchissant la frontière FFI.
///
/// Les erreurs du domaine ne peuvent pas traverser la FFI telles quelles :
/// elles doivent être converties en une représentation simple que Dart peut
/// interpréter. Ce type porte un message déjà lisible par l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct FfiError {
    /// Message prêt à être affiché dans l'interface.
    pub message: String,
}

impl FfiError {
    /// Crée une erreur FFI.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<od_core::DomainError> for FfiError {
    fn from(value: od_core::DomainError) -> Self {
        Self::new(value.to_string())
    }
}

/// État d'un compte, tel que l'interface le représente.
///
/// Miroir sérialisable de [`RegistrationState`] : la FFI ne peut pas exposer
/// directement un `enum` du domaine, qui évoluerait indépendamment du contrat
/// avec Dart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    /// Identifiant du compte.
    pub account_id: String,
    /// Statut, sous forme de chaîne stable (`"unregistered"`, `"registering"`,
    /// `"registered"`, `"failed"`).
    ///
    /// Une chaîne plutôt qu'un `enum` : ajouter un état côté Rust ne cassera
    /// pas le code Dart existant, qui ignore simplement la valeur inconnue.
    pub status: String,
    /// Message lisible, vide si aucun détail n'est utile.
    pub detail: String,
}

impl AccountStatus {
    /// Construit un statut à partir de l'état du domaine.
    #[must_use]
    pub fn from_state(account: &AccountId, state: &RegistrationState) -> Self {
        let (status, detail) = match state {
            RegistrationState::Unregistered => ("unregistered".to_owned(), String::new()),
            RegistrationState::Registering => ("registering".to_owned(), String::new()),
            RegistrationState::Registered { expires_in } => {
                ("registered".to_owned(), format!("{expires_in} s"))
            }
            RegistrationState::Failed { reason, retrying } => {
                let detail = if *retrying {
                    format!("{reason} (nouvelle tentative planifiée)")
                } else {
                    reason.clone()
                };
                ("failed".to_owned(), detail)
            }
        };
        Self {
            account_id: account.to_string(),
            status,
            detail,
        }
    }
}

/// État d'un appel, tel que l'interface le représente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallStatus {
    /// Identifiant de l'appel.
    pub call_id: String,
    /// Correspondant.
    pub remote: String,
    /// État sous forme de chaîne stable.
    pub state: String,
}

impl CallStatus {
    /// Construit un statut d'appel.
    #[must_use]
    pub fn new(call_id: &CallId, remote: impl Into<String>, state: CallState) -> Self {
        Self {
            call_id: call_id.to_string(),
            remote: remote.into(),
            state: state.to_string().to_lowercase(),
        }
    }
}

/// Version de la bibliothèque, exposée à l'interface.
///
/// Sert aussi de **sonde de fumée** : c'est la première fonction appelée par
/// Dart au démarrage. Si elle répond, le pont FFI est opérationnel.
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_exposed_to_the_interface() {
        assert_eq!(version(), "0.1.0");
    }

    #[test]
    fn registration_states_map_to_stable_strings() {
        let account = AccountId::new("a1").expect("identifiant valide");

        let cases = [
            (RegistrationState::Unregistered, "unregistered"),
            (RegistrationState::Registering, "registering"),
            (
                RegistrationState::Registered { expires_in: 300 },
                "registered",
            ),
            (
                RegistrationState::failed_permanent("identifiants invalides"),
                "failed",
            ),
            (
                RegistrationState::failed_retrying("serveur injoignable"),
                "failed",
            ),
        ];

        for (state, expected) in cases {
            let status = AccountStatus::from_state(&account, &state);
            assert_eq!(status.status, expected, "pour {state:?}");
            assert_eq!(status.account_id, "a1");
        }
    }

    #[test]
    fn failure_detail_mentions_pending_retry() {
        let account = AccountId::new("a1").expect("identifiant valide");

        let retrying = AccountStatus::from_state(
            &account,
            &RegistrationState::failed_retrying("serveur injoignable"),
        );
        assert!(retrying.detail.contains("nouvelle tentative"));

        let permanent = AccountStatus::from_state(
            &account,
            &RegistrationState::failed_permanent("identifiants invalides"),
        );
        assert!(!permanent.detail.contains("nouvelle tentative"));
        assert_eq!(permanent.detail, "identifiants invalides");
    }

    #[test]
    fn call_status_uses_lowercase_state_names() {
        let status = CallStatus::new(&CallId::new("c1"), "1002", CallState::Active);
        assert_eq!(status.state, "active");
        assert_eq!(status.remote, "1002");

        let ringing = CallStatus::new(&CallId::new("c2"), "1003", CallState::Ringing);
        assert_eq!(ringing.state, "ringing");
    }

    #[test]
    fn domain_errors_cross_the_boundary_as_readable_messages() {
        let domain_error = od_core::DomainError::EmptyUsername;
        let ffi_error: FfiError = domain_error.into();
        // Le message doit être compréhensible sans connaître le domaine.
        assert!(ffi_error.message.contains("nom d'utilisateur"));
    }
}

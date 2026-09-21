// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Erreurs franchissant la frontière FFI.
//!
//! Les erreurs du domaine ne peuvent pas traverser la FFI telles quelles :
//! elles doivent être converties en une représentation simple que Dart peut
//! interpréter. Ce module est le point de conversion unique.

use thiserror::Error;

/// Erreur présentable à l'interface.
///
/// Porte un message **déjà lisible par l'utilisateur** : la traduction depuis
/// le domaine, le protocole ou la configuration a eu lieu en amont. L'interface
/// n'a donc rien à interpréter, seulement à afficher.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_cross_the_boundary_as_readable_messages() {
        let domain_error = od_core::DomainError::EmptyUsername;
        let ffi_error: FfiError = domain_error.into();
        // Le message doit être compréhensible sans connaître le domaine.
        assert!(ffi_error.message.contains("nom d'utilisateur"));
    }

    #[test]
    fn display_matches_the_message_field() {
        let error = FfiError::new("quelque chose a échoué");
        assert_eq!(error.to_string(), "quelque chose a échoué");
    }
}

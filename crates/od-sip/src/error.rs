// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Erreurs de l'adaptateur de signalisation.
//!
//! Distinctes de [`od_core::DomainError`] : celui-ci décrit une règle métier
//! violée, celles-ci décrivent une défaillance technique — réseau, protocole,
//! configuration. La frontière est volontaire : une erreur de socket ne doit
//! jamais remonter dans le domaine, et une règle métier ne doit jamais
//! dépendre d'un détail de transport.

use thiserror::Error;

/// Défaillance de l'adaptateur SIP.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdaptError {
    /// Le compte fourni par le domaine n'a pas pu être converti.
    ///
    /// Signale un bug interne, pas une erreur utilisateur : le domaine a
    /// validé une donnée que l'adaptateur refuse. Voir `mapping::domain_error`.
    #[error("compte incohérent : {0}")]
    InvalidAccount(String),

    /// Une URI SIP n'a pas pu être construite ou analysée.
    #[error("adresse SIP invalide : {0}")]
    InvalidUri(String),

    /// Défaillance lors d'un échange avec le serveur.
    #[error("{context} : {detail}")]
    Protocol {
        /// Opération en cours au moment de l'échec.
        context: String,
        /// Détail technique, tel que rapporté par rsipstack.
        detail: String,
    },

    /// Le thread de signalisation n'est plus actif.
    ///
    /// Signale que l'agent a été arrêté — après une déconnexion volontaire,
    /// ou à la suite d'une panique dans son thread.
    #[error("le service de signalisation n'est pas démarré")]
    NotRunning,

    /// L'opération demandée n'a pas abouti dans le délai imparti.
    #[error("délai dépassé : {0}")]
    Timeout(String),
}

impl AdaptError {
    /// Crée une erreur d'URI invalide.
    #[must_use]
    pub fn invalid_uri(uri: impl Into<String>) -> Self {
        Self::InvalidUri(uri.into())
    }

    /// Indique si l'opération peut être retentée telle quelle.
    ///
    /// Une erreur de configuration ne se résout pas en réessayant ; une
    /// défaillance réseau, si.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Protocol { .. } | Self::Timeout(_))
    }

    /// Traduit l'erreur en [`od_core::SignalingError`], à destination du domaine.
    ///
    /// C'est le point de conversion entre les deux mondes : à partir d'ici, le
    /// domaine ne voit plus qu'un message lisible et un indicateur de reprise.
    #[must_use]
    pub fn to_signaling_error(&self) -> od_core::SignalingError {
        let message = self.to_string();
        if self.is_retryable() {
            od_core::SignalingError::retryable(message)
        } else {
            od_core::SignalingError::permanent(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_errors_are_not_retryable() {
        // Réessayer avec une URI malformée ne peut pas réussir : la marquer
        // comme réessayable ferait boucler le client indéfiniment.
        assert!(!AdaptError::invalid_uri("pas une uri").is_retryable());
        assert!(!AdaptError::InvalidAccount("champ manquant".to_owned()).is_retryable());
        assert!(!AdaptError::NotRunning.is_retryable());
    }

    #[test]
    fn protocol_and_timeout_errors_are_retryable() {
        assert!(
            AdaptError::Protocol {
                context: "enregistrement".to_owned(),
                detail: "connexion refusée".to_owned(),
            }
            .is_retryable()
        );
        assert!(AdaptError::Timeout("serveur muet".to_owned()).is_retryable());
    }

    #[test]
    fn conversion_to_signaling_error_preserves_retryability() {
        let permanent = AdaptError::invalid_uri("sip:").to_signaling_error();
        assert!(!permanent.retryable);

        let transient = AdaptError::Protocol {
            context: "enregistrement".to_owned(),
            detail: "réseau injoignable".to_owned(),
        }
        .to_signaling_error();
        assert!(transient.retryable);

        // Le message doit rester lisible par un humain côté interface.
        assert!(transient.message.contains("réseau injoignable"));
    }

    #[test]
    fn context_appears_in_the_message() {
        let error = AdaptError::Protocol {
            context: "résolution DNS".to_owned(),
            detail: "hôte introuvable".to_owned(),
        };
        let text = error.to_string();
        assert!(text.contains("résolution DNS"));
        assert!(text.contains("hôte introuvable"));
    }
}

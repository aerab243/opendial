// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! État d'enregistrement d'un compte auprès de son registrar.
//!
//! L'enregistrement SIP (RFC 3261 §10) est ce qui rend un compte **joignable**.
//! Tant qu'il n'a pas abouti, aucun appel entrant ne peut arriver.
//!
//! Ce module ne connaît ni requête `REGISTER`, ni code de statut SIP : il
//! décrit seulement les états observables et les transitions licites. La
//! traduction depuis les réponses du serveur appartient à `od-sip`.

use std::fmt;

use crate::account::AccountId;

/// État d'enregistrement d'un compte.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RegistrationState {
    /// Aucune tentative en cours — état initial, ou après désinscription.
    #[default]
    Unregistered,
    /// Une tentative d'enregistrement est en vol.
    Registering,
    /// Le compte est enregistré et joignable.
    Registered {
        /// Durée d'enregistrement accordée par le serveur, en secondes.
        ///
        /// Peut différer de celle demandée : le serveur impose souvent une
        /// valeur maximale. C'est **cette** valeur qui doit piloter le
        /// rafraîchissement, pas celle du compte.
        expires_in: u32,
    },
    /// L'enregistrement a été refusé par le serveur.
    Failed {
        /// Motif du refus, lisible par un humain.
        ///
        /// Ex. « 401 non autorisé — identifiants invalides ». Le message est
        /// destiné à l'interface : il doit être compréhensible sans connaître
        /// SIP.
        reason: String,
        /// Un nouvel essai est-il planifié ?
        ///
        /// Distingue un échec définitif (identifiants erronés : réessayer ne
        /// sert à rien) d'un échec transitoire (serveur injoignable : une
        /// reprise automatique est pertinente).
        retrying: bool,
    },
}

impl RegistrationState {
    /// Construit un état d'échec avec reprise planifiée.
    #[must_use]
    pub fn failed_retrying(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            retrying: true,
        }
    }

    /// Construit un état d'échec définitif.
    #[must_use]
    pub fn failed_permanent(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            retrying: false,
        }
    }

    /// Indique si le compte est joignable.
    #[must_use]
    pub const fn is_registered(&self) -> bool {
        matches!(self, Self::Registered { .. })
    }

    /// Indique si une tentative est en cours.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Registering)
    }

    /// Renvoie la durée d'enregistrement accordée, si le compte est enregistré.
    #[must_use]
    pub const fn expires_in(&self) -> Option<u32> {
        match self {
            Self::Registered { expires_in } => Some(*expires_in),
            _ => None,
        }
    }
}

impl fmt::Display for RegistrationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unregistered => f.write_str("non enregistré"),
            Self::Registering => f.write_str("enregistrement en cours"),
            Self::Registered { expires_in } => {
                write!(f, "enregistré ({expires_in} s)")
            }
            Self::Failed { reason, retrying } => {
                if *retrying {
                    write!(f, "échec ({reason}), nouvelle tentative planifiée")
                } else {
                    write!(f, "échec ({reason})")
                }
            }
        }
    }
}

/// Changement d'état d'enregistrement, publié vers l'interface.
///
/// Chaque événement porte l'identifiant du compte concerné : l'interface gère
/// plusieurs comptes simultanément et doit pouvoir router sans ambiguïté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationEvent {
    /// Compte concerné.
    pub account: AccountId,
    /// Nouvel état.
    pub state: RegistrationState,
}

impl RegistrationEvent {
    /// Crée un événement d'enregistrement.
    #[must_use]
    pub fn new(account: AccountId, state: RegistrationState) -> Self {
        Self { account, state }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_unregistered() {
        assert_eq!(
            RegistrationState::default(),
            RegistrationState::Unregistered
        );
    }

    #[test]
    fn only_registered_state_is_reachable() {
        let registered = RegistrationState::Registered { expires_in: 300 };
        assert!(registered.is_registered());
        assert!(!registered.is_pending());
        assert_eq!(registered.expires_in(), Some(300));

        let registering = RegistrationState::Registering;
        assert!(!registering.is_registered());
        assert!(registering.is_pending());
        assert_eq!(registering.expires_in(), None);

        assert!(!RegistrationState::Unregistered.is_registered());
        assert_eq!(RegistrationState::Unregistered.expires_in(), None);
    }

    #[test]
    fn failure_distinguishes_transient_from_permanent() {
        let transient = RegistrationState::failed_retrying("serveur injoignable");
        match transient {
            RegistrationState::Failed {
                ref reason,
                retrying,
            } => {
                assert_eq!(reason, "serveur injoignable");
                assert!(retrying);
            }
            _ => panic!("état d'échec attendu"),
        }

        let permanent = RegistrationState::failed_permanent("identifiants invalides");
        match permanent {
            RegistrationState::Failed { retrying, .. } => assert!(!retrying),
            _ => panic!("état d'échec attendu"),
        }

        // Un échec n'est jamais « enregistré », même transitoire.
        assert!(!transient.is_registered());
        assert!(!permanent.is_registered());
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(
            RegistrationState::Unregistered.to_string(),
            "non enregistré"
        );
        assert_eq!(
            RegistrationState::Registered { expires_in: 120 }.to_string(),
            "enregistré (120 s)"
        );
        // Le message d'échec doit rester compréhensible sans connaître SIP.
        let failed = RegistrationState::failed_permanent("identifiants invalides");
        assert!(failed.to_string().contains("identifiants invalides"));
    }

    #[test]
    fn event_carries_account_id_for_routing() {
        let account = AccountId::new("a1").expect("identifiant valide");
        let event = RegistrationEvent::new(account.clone(), RegistrationState::Registering);
        assert_eq!(event.account, account);
        assert!(event.state.is_pending());
    }
}

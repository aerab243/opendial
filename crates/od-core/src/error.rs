// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Erreurs du domaine.
//!
//! Une erreur du domaine décrit **une règle métier violée**, jamais un détail
//! technique. Une socket fermée ou un certificat expiré appartiennent aux
//! adaptateurs (`od-sip`, `od-media`) ; ils sont traduits en erreur de domaine
//! au moment de franchir la frontière.

use thiserror::Error;

/// Erreur de validation d'une donnée du domaine.
///
/// Signale qu'une valeur — identifiant, adresse, port — ne respecte pas une
/// règle métier. Ces erreurs sont détectables sans aucun accès réseau.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// L'identifiant de compte est vide ou composé uniquement d'espaces.
    #[error("l'identifiant de compte est vide")]
    EmptyAccountId,

    /// Le libellé de compte est vide.
    #[error("le libellé de compte est vide")]
    EmptyAccountLabel,

    /// Le nom d'utilisateur SIP est vide.
    #[error("le nom d'utilisateur est vide")]
    EmptyUsername,

    /// L'adresse d'enregistrement ne contient pas de partie domaine.
    ///
    /// Une adresse SIP valide a la forme `utilisateur@domaine`.
    #[error("adresse d'enregistrement invalide, un domaine est attendu : {0}")]
    InvalidAddressOfRecord(String),

    /// Le nom d'hôte du registrar est vide.
    #[error("l'hôte du registrar est vide")]
    EmptyRegistrarHost,

    /// Le nom d'hôte du registrar contient des caractères interdits.
    #[error("hôte de registrar invalide : {0}")]
    InvalidRegistrarHost(String),

    /// La durée d'expiration de l'enregistrement est nulle.
    ///
    /// Un enregistrement de durée nulle est une désinscription explicite
    /// (RFC 3261 §10.2.2) ; elle se demande par `unregister`, jamais par un
    /// `Account` mal formé.
    #[error("la durée d'expiration doit être strictement positive")]
    ZeroExpiry,
}

/// Tentative de transition d'état invalide sur un appel.
///
/// C'est l'erreur la plus importante du domaine : elle rend **impossible** de
/// faire évoluer un appel dans un état incohérent. Recevoir un `200 OK` sur un
/// appel déjà terminé produit cette erreur au lieu d'être silencieusement
/// ignoré — un bug de signalisation devient visible immédiatement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("transition d'appel invalide : {from} → {to} (déclencheur : {trigger})")]
pub struct TransitionError {
    /// État de l'appel avant la tentative.
    pub from: super::call::CallState,
    /// État cible refusé.
    pub to: super::call::CallState,
    /// Opération métier qui a déclenché la tentative.
    pub trigger: TransitionTrigger,
}

/// Opération métier à l'origine d'une transition d'appel.
///
/// Nommer le déclencheur, et pas seulement les états, permet un diagnostic
/// exploitable : « `answer` refusé depuis `Ended` » est actionnable, tandis que
/// « transition invalide » ne l'est pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTrigger {
    /// Appel sortant : envoi de l'`INVITE`.
    Dial,
    /// Réception d'un `180 Ringing`.
    RemoteRinging,
    /// Établissement de la session (réponse `200 OK` + `ACK`).
    Answered,
    /// L'utilisateur local décroche un appel entrant.
    Answer,
    /// Mise en attente locale.
    Hold,
    /// Reprise après mise en attente.
    Unhold,
    /// Fin d'appel, quelle qu'en soit la cause.
    End,
}

impl std::fmt::Display for TransitionTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nom = match self {
            Self::Dial => "dial",
            Self::RemoteRinging => "remote_ringing",
            Self::Answered => "answered",
            Self::Answer => "answer",
            Self::Hold => "hold",
            Self::Unhold => "unhold",
            Self::End => "end",
        };
        f.write_str(nom)
    }
}

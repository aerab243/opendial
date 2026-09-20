// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Appel téléphonique et sa machine à états.
//!
//! C'est le cœur du domaine. Un [`Call`] ne peut évoluer que par des
//! transitions **explicites et validées** : toute tentative illégale produit une
//! [`TransitionError`] au lieu d'être ignorée.
//!
//! ## Pourquoi une machine à états stricte
//!
//! Le protocole SIP est asynchrone et les messages peuvent se croiser : un
//! `BYE` peut arriver pendant qu'un `INVITE` est encore en vol, un `200 OK`
//! peut suivre un `CANCEL`. Un code permissif — « si l'appel est terminé, on
//! ignore l'événement » — masque ces cas et produit des bugs difficiles à
//! reproduire, car ils dépendent du timing réseau.
//!
//! En rendant les transitions illégales **détectables**, un test peut rejouer
//! une séquence de messages dans n'importe quel ordre et vérifier que le
//! domaine réagit correctement, sans réseau ni attente.
//!
//! ## Diagramme
//!
//! ```text
//!                 ┌──────────────────────────────────────┐
//!                 │                                      │
//!                 ▼                                      │
//!   ┌────────┐  dial   ┌─────────┐  remote_ringing  ┌─────────┐
//!   │  Idle  ├────────►│ Dialing ├─────────────────►│ Ringing │
//!   └───┬────┘         └────┬────┘                  └────┬────┘
//!       │                   │                            │
//!       │ incoming_call     │ answered            answered│
//!       ▼                   │                            │
//!  ┌──────────┐  answer     │                            │
//!  │ Incoming ├─────────────┼────────────────────────────┤
//!  └────┬─────┘             │                            │
//!       │                   ▼                            ▼
//!       │              ┌────────────────────────────────────┐
//!       │              │             Active                 │
//!       │              └───────┬────────────────┬───────────┘
//!       │                 hold │                │ unhold
//!       │                      ▼                │
//!       │                 ┌────────┐            │
//!       │                 │  Held  ├────────────┘
//!       │                 └───┬────┘
//!       │                     │
//!       └─────────────────────┴──────────┐
//!                                        │ end (depuis tout état
//!                                        │      non terminal)
//!                                        ▼
//!                                  ┌──────────┐
//!                                  │  Ended   │  (terminal)
//!                                  └──────────┘
//! ```

use std::fmt;

use crate::error::{TransitionError, TransitionTrigger};

/// Identifiant unique d'un appel au sein de l'application.
///
/// Distinct de l'identifiant de dialogue SIP : le domaine ne doit pas dépendre
/// d'un concept de protocole. L'adaptateur maintient la correspondance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CallId(String);

impl CallId {
    /// Crée un identifiant d'appel.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Renvoie l'identifiant sous forme de chaîne.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Sens de l'appel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallDirection {
    /// Appel sortant, initié par l'utilisateur local.
    Outgoing,
    /// Appel entrant, reçu du réseau.
    Incoming,
}

/// État d'un appel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallState {
    /// Aucun appel — état initial, avant toute numérotation.
    Idle,
    /// Appel entrant reçu, en attente de décision locale.
    Incoming,
    /// Appel sortant : `INVITE` envoyé, en attente de réponse.
    Dialing,
    /// Le correspondant sonne (`180 Ringing` ou `183 Session Progress`).
    Ringing,
    /// Conversation établie, audio bidirectionnel.
    Active,
    /// Appel en attente : la session média est suspendue.
    Held,
    /// Appel terminé. État **terminal** : aucune transition n'en sort.
    Ended,
}

impl CallState {
    /// Indique si l'état est terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Ended)
    }

    /// Indique si une conversation est en cours, active ou en attente.
    #[must_use]
    pub const fn is_established(self) -> bool {
        matches!(self, Self::Active | Self::Held)
    }

    /// Indique si l'appel occupe une ligne — non terminé.
    ///
    /// C'est la question que se pose la gestion de lignes multiples : un appel
    /// en `Dialing` occupe déjà une ligne, même s'il n'est pas encore établi.
    #[must_use]
    pub const fn is_live(self) -> bool {
        !matches!(self, Self::Idle | Self::Ended)
    }

    /// Indique si l'état représente un appel entrant non encore accepté.
    #[must_use]
    pub const fn is_awaiting_decision(self) -> bool {
        matches!(self, Self::Incoming)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Incoming => "Incoming",
            Self::Dialing => "Dialing",
            Self::Ringing => "Ringing",
            Self::Active => "Active",
            Self::Held => "Held",
            Self::Ended => "Ended",
        }
    }
}

impl fmt::Display for CallState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Cause de fin d'appel.
///
/// Distincte de l'état [`CallState::Ended`], qui indique seulement *que*
/// l'appel est fini. La cause est ce qui permet de renseigner l'historique et
/// d'afficher « Appel manqué » plutôt que « Appel terminé ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    /// L'utilisateur local a raccroché.
    LocalHangup,
    /// Le correspondant a raccroché.
    RemoteHangup,
    /// Le correspondant a refusé l'appel (occupé, rejet explicite).
    Rejected,
    /// Appel entrant resté sans réponse.
    Missed,
    /// Appel sortant resté sans réponse.
    Unanswered,
    /// L'appel n'a pas pu aboutir.
    Failed {
        /// Motif lisible par un humain.
        reason: String,
    },
    /// L'appel a été annulé par l'émetteur avant d'aboutir.
    Cancelled,
}

impl EndReason {
    /// Indique si la cause traduit un appel manqué.
    ///
    /// Utilisé par l'historique pour distinguer visuellement un appel manqué
    /// d'un appel simplement terminé.
    #[must_use]
    pub const fn is_missed(&self) -> bool {
        matches!(self, Self::Missed | Self::Unanswered)
    }
}

impl fmt::Display for EndReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalHangup => f.write_str("raccroché par l'appelant"),
            Self::RemoteHangup => f.write_str("raccroché par le correspondant"),
            Self::Rejected => f.write_str("appel refusé"),
            Self::Missed => f.write_str("appel manqué"),
            Self::Unanswered => f.write_str("sans réponse"),
            Self::Failed { reason } => write!(f, "échec : {reason}"),
            Self::Cancelled => f.write_str("appel annulé"),
        }
    }
}

/// Appel téléphonique.
///
/// Le type est volontairement **sans horodatage** : il ne lit jamais l'horloge.
/// La durée d'un appel est calculée par l'appelant, à partir d'événements
/// horodatés à la frontière du domaine. C'est ce qui rend les tests
/// parfaitement déterministes : rejouer la même séquence produit toujours le
/// même résultat, quel que soit le moment de l'exécution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    id: CallId,
    direction: CallDirection,
    state: CallState,
    /// Correspondant : numéro ou URI appelé.
    remote: String,
    /// Cause de fin, renseignée uniquement lorsque l'état est `Ended`.
    end_reason: Option<EndReason>,
    /// Motif d'erreur d'une transition refusée, à destination de l'interface.
    last_error: Option<String>,
}

impl Call {
    /// Crée un appel sortant à l'état [`CallState::Idle`].
    #[must_use]
    pub fn outgoing(id: CallId, remote: impl Into<String>) -> Self {
        Self {
            id,
            direction: CallDirection::Outgoing,
            state: CallState::Idle,
            remote: remote.into(),
            end_reason: None,
            last_error: None,
        }
    }

    /// Crée un appel entrant à l'état [`CallState::Incoming`].
    ///
    /// Un appel entrant naît dans l'état `Incoming` : il existe parce que le
    /// réseau l'a signalé, pas parce que l'utilisateur l'a demandé.
    #[must_use]
    pub fn incoming(id: CallId, remote: impl Into<String>) -> Self {
        Self {
            id,
            direction: CallDirection::Incoming,
            state: CallState::Incoming,
            remote: remote.into(),
            end_reason: None,
            last_error: None,
        }
    }

    /// Identifiant de l'appel.
    #[must_use]
    pub const fn id(&self) -> &CallId {
        &self.id
    }

    /// Sens de l'appel.
    #[must_use]
    pub const fn direction(&self) -> CallDirection {
        self.direction
    }

    /// État courant.
    #[must_use]
    pub const fn state(&self) -> CallState {
        self.state
    }

    /// Correspondant.
    #[must_use]
    pub fn remote(&self) -> &str {
        &self.remote
    }

    /// Cause de fin, si l'appel est terminé.
    #[must_use]
    pub const fn end_reason(&self) -> Option<&EndReason> {
        self.end_reason.as_ref()
    }

    /// Dernière transition refusée, si l'appelant en a ignoré une.
    ///
    /// Permet à l'interface de signaler une incohérence de signalisation sans
    /// interrompre l'appel en cours.
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Applique une transition, ou renvoie une erreur si elle est illégale.
    ///
    /// C'est **le seul point de mutation de l'état**. Centraliser la validation
    /// garantit qu'aucun chemin de code ne peut contourner la machine à états.
    fn transition(
        &mut self,
        target: CallState,
        trigger: TransitionTrigger,
    ) -> Result<(), TransitionError> {
        if self.is_allowed(target, trigger) {
            self.state = target;
            self.last_error = None;
            return Ok(());
        }

        let error = TransitionError {
            from: self.state,
            to: target,
            trigger,
        };
        // L'erreur est mémorisée avant d'être propagée : une signalisation
        // incohérente doit rester visible, même si l'appelant ignore le
        // `Result`.
        self.last_error = Some(error.to_string());
        Err(error)
    }

    /// Indique si une transition est licite depuis l'état courant.
    ///
    /// La validation porte sur le **couple** (état cible, déclencheur), et non
    /// sur le seul état cible. C'est nécessaire car deux opérations distinctes
    /// mènent au même état : [`Call::answer`] (décrocher un appel entrant) et
    /// [`Call::answered`] (constater que le correspondant a répondu) visent
    /// tous deux [`CallState::Active`]. Sans le déclencheur, on ne pourrait pas
    /// refuser un « décrochage » sur un appel qu'on a soi-même émis.
    #[must_use]
    pub const fn is_allowed(&self, target: CallState, trigger: TransitionTrigger) -> bool {
        use CallState::{Active, Dialing, Ended, Held, Idle, Incoming, Ringing};
        use TransitionTrigger as T;

        // Aucune transition ne sort d'un état terminal.
        if matches!(self.state, Ended) {
            return false;
        }

        // La fin d'appel est toujours permise depuis un état non terminal.
        if matches!(target, Ended) {
            return matches!(trigger, T::End);
        }

        matches!(
            (self.state, target, trigger),
            // Appel sortant : numérotation, puis progression.
            (Idle, Dialing, T::Dial)
                | (Dialing, Ringing, T::RemoteRinging)
                // L'établissement est le seul cas où deux états d'origine
                // partagent le même déclencheur : un serveur peut répondre
                // directement par 200 OK (sans 180 Ringing intermédiaire).
                | (Dialing | Ringing, Active, T::Answered)
                // Appel entrant : décrochage local uniquement.
                | (Incoming, Active, T::Answer)
                // Mise en attente et reprise.
                | (Active, Held, T::Hold)
                | (Held, Active, T::Unhold)
        )
    }

    /// Numérote le correspondant : passe de `Idle` à `Dialing`.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`TransitionError`] si l'appel n'est pas à l'état `Idle` — par
    /// exemple si un appel entrant attend déjà une décision.
    pub fn dial(&mut self) -> Result<(), TransitionError> {
        self.transition(CallState::Dialing, TransitionTrigger::Dial)
    }

    /// Signale que le correspondant sonne (`180 Ringing`).
    ///
    /// # Erreurs
    ///
    /// Renvoie [`TransitionError`] si l'appel n'est pas en cours de
    /// numérotation.
    pub fn remote_ringing(&mut self) -> Result<(), TransitionError> {
        self.transition(CallState::Ringing, TransitionTrigger::RemoteRinging)
    }

    /// Établit la conversation.
    ///
    /// Couvre deux cas : la réponse à un `INVITE` sortant, et le décrochage
    /// d'un appel entrant. Les deux mènent à [`CallState::Active`].
    ///
    /// # Erreurs
    ///
    /// Renvoie [`TransitionError`] si l'appel est `Idle` — on ne peut pas
    /// établir un appel qui n'a pas commencé.
    pub fn answered(&mut self) -> Result<(), TransitionError> {
        self.transition(CallState::Active, TransitionTrigger::Answered)
    }

    /// L'utilisateur local décroche un appel entrant.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`TransitionError`] si l'appel n'est pas à l'état `Incoming`.
    /// Décrocher un appel sortant n'a pas de sens et doit être signalé.
    pub fn answer(&mut self) -> Result<(), TransitionError> {
        self.transition(CallState::Active, TransitionTrigger::Answer)
    }

    /// Met l'appel en attente.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`TransitionError`] si l'appel n'est pas actif.
    pub fn hold(&mut self) -> Result<(), TransitionError> {
        self.transition(CallState::Held, TransitionTrigger::Hold)
    }

    /// Reprend un appel en attente.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`TransitionError`] si l'appel n'est pas en attente.
    pub fn unhold(&mut self) -> Result<(), TransitionError> {
        self.transition(CallState::Active, TransitionTrigger::Unhold)
    }

    /// Termine l'appel, quelle qu'en soit la cause.
    ///
    /// Sans effet mais **non fautif** si l'appel est déjà terminé : c'est
    /// délibéré, car un raccrochage peut être demandé deux fois (l'utilisateur
    /// clique, puis le réseau confirme). Raccrocher est idempotent.
    ///
    /// # Erreurs
    ///
    /// Ne renvoie jamais d'erreur : terminer un appel est toujours licite
    /// depuis un état non terminal, et sans effet depuis `Ended`.
    pub fn end(&mut self, reason: EndReason) -> Result<(), TransitionError> {
        if self.state.is_terminal() {
            return Ok(());
        }
        self.transition(CallState::Ended, TransitionTrigger::End)?;
        self.end_reason = Some(reason);
        Ok(())
    }
}

/// Événement publié vers l'interface à chaque évolution d'un appel.
///
/// L'interface Flutter ne lit jamais l'état interne : elle consomme ce flux et
/// se contente de refléter ce que le domaine décide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEvent {
    /// Appel concerné.
    pub id: CallId,
    /// État résultant.
    pub state: CallState,
    /// Cause de fin, présente uniquement sur un événement terminal.
    pub end_reason: Option<EndReason>,
}

impl CallEvent {
    /// Crée un événement à partir de l'état courant d'un appel.
    #[must_use]
    pub fn from_call(call: &Call) -> Self {
        Self {
            id: call.id.clone(),
            state: call.state,
            end_reason: call.end_reason.clone(),
        }
    }

    /// Indique si cet événement clôt l'appel.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> CallId {
        CallId::new("call-1")
    }

    fn outgoing() -> Call {
        Call::outgoing(id(), "1002")
    }

    fn incoming() -> Call {
        Call::incoming(id(), "1001")
    }

    // --- Parcours nominal ------------------------------------------------

    #[test]
    fn outgoing_call_reaches_active_through_ringing() {
        let mut call = outgoing();
        assert_eq!(call.state(), CallState::Idle);

        call.dial().expect("dial depuis Idle");
        assert_eq!(call.state(), CallState::Dialing);

        call.remote_ringing().expect("sonnerie depuis Dialing");
        assert_eq!(call.state(), CallState::Ringing);

        call.answered().expect("réponse depuis Ringing");
        assert_eq!(call.state(), CallState::Active);
        assert!(call.state().is_established());
        assert!(call.last_error().is_none());
    }

    #[test]
    fn outgoing_call_can_be_answered_without_ringing() {
        // Certains serveurs répondent directement par 200 OK : l'état Ringing
        // n'est jamais observé. Le domaine doit l'accepter.
        let mut call = outgoing();
        call.dial().expect("dial");
        call.answered().expect("réponse directe depuis Dialing");
        assert_eq!(call.state(), CallState::Active);
    }

    #[test]
    fn incoming_call_is_answered() {
        let mut call = incoming();
        assert_eq!(call.state(), CallState::Incoming);

        call.answer().expect("décrochage");
        assert_eq!(call.state(), CallState::Active);
    }

    #[test]
    fn hold_and_unhold_round_trip() {
        let mut call = active_call();

        call.hold().expect("mise en attente");
        assert_eq!(call.state(), CallState::Held);
        assert!(
            call.state().is_established(),
            "un appel en attente reste établi"
        );

        call.unhold().expect("reprise");
        assert_eq!(call.state(), CallState::Active);
    }

    fn active_call() -> Call {
        let mut call = outgoing();
        call.dial().expect("dial");
        call.answered().expect("réponse");
        call
    }

    // --- Transitions refusées --------------------------------------------

    #[test]
    fn cannot_dial_an_incoming_call() {
        let mut call = incoming();
        let error = call
            .dial()
            .expect_err("dial doit échouer sur un appel entrant");

        assert_eq!(error.from, CallState::Incoming);
        assert_eq!(error.to, CallState::Dialing);
        assert_eq!(error.trigger, TransitionTrigger::Dial);
        // L'état n'a pas bougé.
        assert_eq!(call.state(), CallState::Incoming);
    }

    #[test]
    fn cannot_answer_an_outgoing_call() {
        let mut call = outgoing();
        call.dial().expect("dial");

        // Décrocher un appel qu'on a soi-même émis est une incohérence : elle
        // doit être signalée, pas ignorée.
        let error = call.answer().expect_err("answer doit échouer");
        assert_eq!(error.trigger, TransitionTrigger::Answer);
        assert_eq!(call.state(), CallState::Dialing);
    }

    #[test]
    fn answer_and_answered_are_distinguished_despite_sharing_a_target() {
        // `answer` (décrocher) et `answered` (constater la réponse distante)
        // mènent tous deux à `Active`. La validation doit porter sur le
        // déclencheur, sinon un appel sortant pourrait être « décroché » et un
        // appel entrant « répondu » par le réseau.
        let mut outgoing_call = outgoing();
        outgoing_call.dial().expect("dial");
        assert!(
            outgoing_call.answer().is_err(),
            "un appel sortant ne se décroche pas"
        );
        assert!(outgoing_call.answered().is_ok(), "il aboutit par réponse");

        let mut incoming_call = incoming();
        assert!(
            incoming_call.answered().is_err(),
            "un appel entrant n'aboutit pas par réponse distante"
        );
        assert!(incoming_call.answer().is_ok(), "il se décroche localement");
    }

    #[test]
    fn ringing_cannot_be_entered_by_a_distant_answer() {
        // Régression : un regroupement de motifs trop large avait rendu
        // `Dialing → Ringing` accessible via `Answered`. Les tuples état/état/
        // déclencheur ne doivent jamais être regroupés approximativement.
        let mut call = outgoing();
        call.dial().expect("dial");

        assert!(
            !call.is_allowed(CallState::Ringing, TransitionTrigger::Answered),
            "on n'entre pas en sonnerie par une réponse distante"
        );
        // Le chemin correct reste ouvert.
        assert!(call.is_allowed(CallState::Ringing, TransitionTrigger::RemoteRinging));
        assert!(call.is_allowed(CallState::Active, TransitionTrigger::Answered));
    }

    #[test]
    fn is_allowed_requires_the_matching_trigger() {
        let active = active_call();
        // Même état cible, déclencheur inadapté : refusé.
        assert!(active.is_allowed(CallState::Ended, TransitionTrigger::End));
        assert!(!active.is_allowed(CallState::Ended, TransitionTrigger::Hold));
        assert!(!active.is_allowed(CallState::Held, TransitionTrigger::Unhold));
        assert!(active.is_allowed(CallState::Held, TransitionTrigger::Hold));
    }

    #[test]
    fn cannot_establish_an_idle_call() {
        let mut call = outgoing();
        // Un appel qui n'a jamais été numéroté ne peut pas devenir actif.
        assert!(call.answered().is_err());
        assert_eq!(call.state(), CallState::Idle);
    }

    #[test]
    fn cannot_hold_a_dialing_call() {
        let mut call = outgoing();
        call.dial().expect("dial");
        assert!(call.hold().is_err());
        assert_eq!(call.state(), CallState::Dialing);
    }

    #[test]
    fn cannot_unhold_an_active_call() {
        let mut call = active_call();
        // Reprendre un appel déjà actif n'a pas de sens.
        assert!(call.unhold().is_err());
        assert_eq!(call.state(), CallState::Active);
    }

    // --- État terminal ---------------------------------------------------

    #[test]
    fn ended_is_terminal_and_absorbs_everything() {
        let mut call = active_call();
        call.end(EndReason::LocalHangup).expect("fin d'appel");
        assert_eq!(call.state(), CallState::Ended);
        assert!(call.state().is_terminal());
        assert!(!call.state().is_live());

        // Aucune transition ne sort d'un état terminal.
        assert!(call.dial().is_err());
        assert!(call.answer().is_err());
        assert!(call.hold().is_err());
        assert!(call.unhold().is_err());
        assert!(call.answered().is_err());
        assert_eq!(call.state(), CallState::Ended);
    }

    #[test]
    fn end_is_idempotent() {
        // L'utilisateur raccroche, puis le réseau confirme : raccrocher deux
        // fois ne doit pas être une erreur.
        let mut call = active_call();
        call.end(EndReason::LocalHangup).expect("première fin");
        call.end(EndReason::RemoteHangup)
            .expect("seconde fin tolérée");

        assert_eq!(call.state(), CallState::Ended);
        // La cause d'origine est conservée : c'est la première qui fait foi.
        assert_eq!(call.end_reason(), Some(&EndReason::LocalHangup));
    }

    #[test]
    fn can_end_from_every_non_terminal_state() {
        for build in [
            || outgoing(),
            || incoming(),
            || {
                let mut c = outgoing();
                c.dial().expect("dial");
                c
            },
            || {
                let mut c = outgoing();
                c.dial().expect("dial");
                c.remote_ringing().expect("sonnerie");
                c
            },
            active_call,
            || {
                let mut c = active_call();
                c.hold().expect("attente");
                c
            },
        ] {
            let mut call = build();
            let before = call.state();
            call.end(EndReason::LocalHangup)
                .unwrap_or_else(|e| panic!("fin impossible depuis {before} : {e}"));
            assert_eq!(call.state(), CallState::Ended, "depuis {before}");
        }
    }

    // --- Sécurité des identifiants ---------------------------------------

    #[test]
    fn failed_transition_is_recorded_for_diagnostics() {
        let mut call = incoming();
        assert!(call.last_error().is_none());

        let _ = call.dial();

        let recorded = call.last_error().expect("erreur mémorisée");
        assert!(recorded.contains("Incoming"));
        assert!(recorded.contains("Dialing"));
        assert!(recorded.contains("dial"));
    }

    #[test]
    fn successful_transition_clears_previous_error() {
        let mut call = incoming();
        let _ = call.dial(); // échoue et mémorise
        assert!(call.last_error().is_some());

        call.answer().expect("décrochage");
        assert!(call.last_error().is_none(), "l'erreur doit être effacée");
    }

    // --- Événements ------------------------------------------------------

    #[test]
    fn events_reflect_call_state() {
        let mut call = active_call();
        let event = CallEvent::from_call(&call);
        assert_eq!(event.id, *call.id());
        assert_eq!(event.state, CallState::Active);
        assert!(event.end_reason.is_none());
        assert!(!event.is_terminal());

        call.end(EndReason::Missed).expect("fin");
        let event = CallEvent::from_call(&call);
        assert!(event.is_terminal());
        assert_eq!(event.end_reason, Some(EndReason::Missed));
    }

    #[test]
    fn missed_reason_is_flagged_for_history() {
        assert!(EndReason::Missed.is_missed());
        assert!(EndReason::Unanswered.is_missed());
        assert!(!EndReason::LocalHangup.is_missed());
        assert!(!EndReason::Rejected.is_missed());
    }

    #[test]
    fn failure_reason_is_human_readable() {
        let reason = EndReason::Failed {
            reason: "503 Service Unavailable".to_owned(),
        };
        let text = reason.to_string();
        assert!(text.contains("503 Service Unavailable"));
    }

    // --- Invariants ------------------------------------------------------

    #[test]
    fn call_direction_is_immutable() {
        let call = outgoing();
        assert_eq!(call.direction(), CallDirection::Outgoing);
        let call = incoming();
        assert_eq!(call.direction(), CallDirection::Incoming);
    }

    #[test]
    fn state_predicates_are_coherent() {
        assert!(!CallState::Idle.is_live());
        assert!(!CallState::Ended.is_live());
        assert!(CallState::Incoming.is_live());
        assert!(CallState::Dialing.is_live());
        assert!(CallState::Active.is_live());
        assert!(CallState::Held.is_live());

        assert!(CallState::Active.is_established());
        assert!(CallState::Held.is_established());
        assert!(!CallState::Dialing.is_established());

        assert!(CallState::Incoming.is_awaiting_decision());
        assert!(!CallState::Ringing.is_awaiting_decision());
    }
}

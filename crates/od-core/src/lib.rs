// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Avis volontairement conforme à l'« Exhibit A » de la MPL-2.0, et NON à
// l'« Exhibit B » : opendial doit rester compatible GPL/LGPL/AGPL via la
// clause §3.3. Voir docs/adr/0003.

//! # od-core — le domaine d'opendial
//!
//! Ce crate contient le **domaine métier** du softphone : ce qu'est un compte,
//! un appel, une session, et comment ces objets évoluent. Il ne sait rien de
//! SIP, de RTP, du système d'exploitation ou de Flutter.
//!
//! ## Pourquoi cette séparation
//!
//! Un softphone doit pouvoir être **testé sans réseau, sans carte son et sans
//! interface graphique**. C'est précisément ce que MicroSIP ne permettait pas :
//! son modèle de données héritait de `CString` (MFC), si bien qu'un bug dans la
//! logique d'appel exigeait d'ouvrir une fenêtre Windows pour être reproduit
//! (voir `docs/adr/0001`).
//!
//! Ici, un scénario d'appel complet — décrochage, mise en attente, transfert,
//! terminaison — s'exécute en quelques microsecondes dans un test unitaire.
//!
//! ## Frontières
//!
//! ```text
//!        ┌──────────────────────────────────────────┐
//!        │  od-core  (ce crate)                     │
//!        │  Types purs · machines à états · ports   │
//!        │  Aucune I/O · aucun runtime · aucun SIP  │
//!        └───────────────┬──────────────────────────┘
//!                        │ traits-ports (inversés)
//!         ┌──────────────┼──────────────┐
//!         ▼              ▼              ▼
//!      od-sip        od-media       od-audio
//!    (rsipstack)     (rustrtc)        (cpal)
//! ```
//!
//! Les crates d'adaptation **dépendent de `od-core`**, jamais l'inverse. Le
//! domaine définit les traits qu'il attend ; chaque adaptateur les implémente.
//! Remplacer `rsipstack` par un autre stack SIP ne touche donc ni le domaine,
//! ni l'interface Flutter — c'est la mitigation du risque identifiée dans
//! `docs/adr/0002`.
//!
//! ## Principes
//!
//! - **Aucune I/O.** Un type du domaine ne lit ni n'écrit jamais rien.
//! - **Aucune horloge implicite.** Les états ne portent pas d'horodatage : le
//!   temps est fourni de l'extérieur, ce qui rend les tests déterministes.
//! - **Transitions explicites.** Un appel ne peut pas passer d'un état à un
//!   autre sans que la transition soit déclarée et validée. Un `200 OK` reçu
//!   sur un appel déjà terminé est une erreur, pas un silence.

pub mod account;
pub mod call;
pub mod error;
pub mod ports;
pub mod registration;

pub use account::{Account, AccountId, Credentials, Expiry, Registrar, Transport};
pub use call::{Call, CallDirection, CallEvent, CallId, CallState, EndReason};
pub use error::{DomainError, TransitionError};
// Les traits-ports et leurs erreurs sont réexportés à la racine : ce sont les
// types que les adaptateurs implémentent et manipulent en premier.
pub use ports::{
    MediaError, MediaPort, MediaSessionInfo, SignalingError, SignalingPort,
};
pub use registration::{RegistrationEvent, RegistrationState};

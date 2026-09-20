// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Avis conforme à l'« Exhibit A » de la MPL-2.0, et NON à l'« Exhibit B » :
// opendial doit rester compatible GPL/LGPL/AGPL via la clause §3.3. Voir
// docs/adr/0003.

//! # od-sip — adaptateur de signalisation SIP
//!
//! Implémente les traits-ports de `od-core` à l'aide de `rsipstack`. C'est le
//! seul crate du projet qui connaisse SIP : le domaine ignore tout du
//! protocole, et l'interface Flutter n'en voit jamais un seul octet.
//!
//! ## Pourquoi cette frontière
//!
//! `rsipstack` est une crate **0.x maintenue par un seul auteur**. Si elle est
//! abandonnée, on écrit un nouvel adaptateur derrière les mêmes traits — le
//! domaine, ses tests et l'interface restent intacts. C'est précisément ce que
//! MicroSIP ne pouvait pas faire, soudé à PJSIP par `pjsua_internal.h`
//! (voir `docs/adr/0001`).
//!
//! ## Le modèle d'exécution
//!
//! rsipstack est asynchrone ; le domaine ne l'est pas. Cet adaptateur fait le
//! pont en isolant une **instance tokio dédiée dans son propre thread** :
//!
//! ```text
//!   Thread appelant (Flutter)     Thread dédié od-sip
//!   ─────────────────────────     ─────────────────────
//!   register(account) ──────►  canal ──────► runtime tokio
//!   blocage sur réponse  ◄──── canal ◄────── Registration::register().await
//! ```
//!
//! Pourquoi un thread dédié plutôt que le runtime de l'appelant :
//!
//! - **Le domaine reste sans async.** Ses traits sont synchrones par choix
//!   délibéré : cela lui évite de dépendre de tokio, et rend ses tests
//!   triviaux.
//! - **Le runtime survit aux appels.** rsipstack doit rester actif en
//!   permanence pour répondre aux requêtes entrantes et rafraîchir
//!   l'enregistrement. Un runtime créé par appel mourrait à chaque retour.
//! - **Flutter ne voit pas d'async.** L'interface appelle une fonction
//!   bloquante courte ; la FFI la déporte sur un fil d'exécution dédié.

mod agent;
mod error;
mod mapping;

pub use agent::SipAgent;
pub use error::AdaptError;

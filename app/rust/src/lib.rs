// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! # opendial_bridge — pont entre Flutter et le domaine
//!
//! Ce crate est le **point d'entrée de la bibliothèque native** chargée par
//! l'application Flutter. Il expose les fonctions de [`api`] via
//! `flutter_rust_bridge`, qui génère le code d'appel côté Dart.
//!
//! ## Place dans l'architecture
//!
//! ```text
//!   Flutter (Dart)
//!       │  appels générés par flutter_rust_bridge
//!       ▼
//!   opendial_bridge   ← ce crate : traduction uniquement
//!       │
//!       ▼
//!   od-ffi  →  od-core  (+ adaptateurs od-sip, od-media…)
//! ```
//!
//! Le pont ne contient **aucune logique métier** : il rend simplement le
//! domaine appelable depuis Dart. Toute règle qui apparaîtrait ici échapperait
//! aux tests de `od-core` et deviendrait invérifiable sans lancer l'interface —
//! précisément le défaut qui rend MicroSIP intestable (voir `docs/adr/0001`).

pub mod api;

// Le code de liaison généré par flutter_rust_bridge. Il est reproduit par
// `flutter_rust_bridge_codegen generate` et exclu du dépôt : le modifier à la
// main serait perdu à la génération suivante.
//
// Les lints sont levés LOCALEMENT pour ce module, et pour lui seul :
//
// - `unsafe_code` : le code de liaison FFI utilise légitimement `unsafe` pour
//   lire les pointeurs fournis par Dart. C'est sa raison d'être. La politique
//   du projet vise NOTRE code, pas celui d'un outil tiers. (`deny` et non
//   `forbid` : `forbid` interdit tout `allow`, même ciblé.)
// - `clippy::pedantic` : les conversions numériques brutes et les littéraux
//   sans séparateurs sont normaux dans du code de sérialisation généré. Les
//   corriger à la main serait vain — la prochaine génération les rétablirait.
#[allow(unsafe_code)]
#[allow(clippy::pedantic, clippy::all)]
#[cfg(not(target_family = "wasm"))]
mod frb_generated;

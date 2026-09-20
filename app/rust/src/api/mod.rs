// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! # Surface exposée à Dart
//!
//! Chaque fonction publique de ce module devient une fonction Dart après
//! génération. C'est donc un **contrat stable** avec l'interface.
//!
//! ## Règle : aucune logique métier ici
//!
//! Ces fonctions se contentent de traduire un appel Dart en opération du
//! domaine, et un résultat du domaine en type sérialisable. Toute validation
//! ou décision doit vivre dans `od-core`, où elle est testable sans Flutter.
//!
//! ## Organisation
//!
//! - [`system`] — diagnostic et information de version.
//! - Les modules métier (`account`, `call`) accompagnent les phases 1 et 2.

pub mod system;

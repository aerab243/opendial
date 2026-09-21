// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Avis conforme à l'« Exhibit A » de la MPL-2.0, et NON à l'« Exhibit B » :
// opendial doit rester compatible GPL/LGPL/AGPL via la clause §3.3. Voir
// docs/adr/0003.

//! # od-ffi — façade entre Flutter et le domaine
//!
//! Ce crate est la **seule** surface que Flutter peut atteindre. Il expose des
//! types sérialisables et des fonctions simples, et traduit les appels vers
//! `od-core` et ses adaptateurs.
//!
//! ## Règle absolue : aucune logique métier ici
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
//! d'architecture : elle échapperait aux tests du domaine et deviendrait
//! invérifiable sans lancer Flutter. C'est exactement le travers qui rend
//! MicroSIP intestable (voir `docs/adr/0001`).
//!
//! ## Organisation
//!
//! - [`accounts`] — cycle de vie des comptes : ajout, enregistrement, retrait.
//! - [`error`] — erreur franchissant la frontière FFI.
//!
//! ## État actuel
//!
//! Phase 1 : les comptes et leur enregistrement. Les appels arrivent en
//! Phase 2.

pub mod accounts;
pub mod error;

pub use accounts::{AccountDraft, AccountInfo, AccountService};
pub use error::FfiError;

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
}

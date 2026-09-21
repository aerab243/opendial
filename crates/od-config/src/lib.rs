// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! # od-config — persistance des comptes et des préférences
//!
//! Phase 1 : les comptes vivent **en mémoire uniquement**. La persistance sur
//! disque, avec chiffrement des mots de passe, arrive en Phase 4 — elle
//! soulève des questions qui méritent leur propre décision : emplacement,
//! format, et surtout protection par le trousseau du système plutôt qu'un
//! fichier en clair.
//!
//! Ce module existe dès maintenant pour que `od-ffi` s'appuie sur une
//! abstraction stable : le jour où les comptes seront écrits sur disque,
//! seule cette implémentation changera, et ni la FFI ni l'interface ne s'en
//! apercevront.

use std::collections::HashMap;

use od_core::{Account, AccountId};
use thiserror::Error;

/// Erreur de gestion de la configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    /// Un compte portant cet identifiant existe déjà.
    #[error("un compte portant l'identifiant « {0} » existe déjà")]
    DuplicateAccount(AccountId),

    /// Aucun compte ne porte cet identifiant.
    #[error("aucun compte ne porte l'identifiant « {0} »")]
    UnknownAccount(AccountId),
}

/// Référentiel de comptes.
///
/// Volontairement minimal en Phase 1 : stockage en mémoire, sans sérialisation
/// ni chiffrement.
#[derive(Debug, Default)]
pub struct AccountStore {
    accounts: HashMap<AccountId, Account>,
}

impl AccountStore {
    /// Crée un référentiel vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un compte.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`ConfigError::DuplicateAccount`] si l'identifiant est déjà
    /// pris. Refuser explicitement vaut mieux qu'écraser en silence : une
    /// interface qui ajoute deux fois le même compte est un bug, et le masquer
    /// ferait perdre la configuration saisie sans que l'utilisateur le sache.
    pub fn add(&mut self, account: Account) -> Result<(), ConfigError> {
        if self.accounts.contains_key(&account.id) {
            return Err(ConfigError::DuplicateAccount(account.id));
        }
        self.accounts.insert(account.id.clone(), account);
        Ok(())
    }

    /// Remplace un compte existant, ou l'ajoute s'il est inconnu.
    ///
    /// Utile pour modifier les paramètres d'un compte sans changer son
    /// identifiant.
    pub fn upsert(&mut self, account: Account) {
        self.accounts.insert(account.id.clone(), account);
    }

    /// Retire un compte.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`ConfigError::UnknownAccount`] si l'identifiant est inconnu.
    pub fn remove(&mut self, id: &AccountId) -> Result<Account, ConfigError> {
        self.accounts
            .remove(id)
            .ok_or_else(|| ConfigError::UnknownAccount(id.clone()))
    }

    /// Renvoie un compte par son identifiant.
    #[must_use]
    pub fn get(&self, id: &AccountId) -> Option<&Account> {
        self.accounts.get(id)
    }

    /// Renvoie tous les comptes, triés par libellé pour un affichage stable.
    ///
    /// Le tri est fait ici plutôt que dans l'interface : sans lui, l'ordre
    /// d'un `HashMap` varie d'une exécution à l'autre, et la liste des comptes
    /// se réorganiserait à chaque démarrage.
    #[must_use]
    pub fn all(&self) -> Vec<&Account> {
        let mut accounts: Vec<&Account> = self.accounts.values().collect();
        accounts.sort_by(|a, b| a.label.cmp(&b.label));
        accounts
    }

    /// Nombre de comptes enregistrés.
    #[must_use]
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Indique si le référentiel est vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Credentials, Registrar};

    fn account(id: &str, label: &str) -> Account {
        Account::new(
            AccountId::new(id).expect("identifiant valide"),
            label,
            "1001@pbx.example.com",
            Credentials::new("1001", "secret").expect("identifiants valides"),
            Registrar::new("pbx.example.com").expect("hôte valide"),
        )
        .expect("compte valide")
    }

    #[test]
    fn add_then_get_returns_the_account() {
        let mut store = AccountStore::new();
        let id = AccountId::new("a1").expect("identifiant valide");
        store.add(account("a1", "Travail")).expect("ajout");

        let found = store.get(&id).expect("compte présent");
        assert_eq!(found.label, "Travail");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn duplicate_id_is_refused_explicitly() {
        // Écraser en silence ferait perdre la configuration saisie sans que
        // l'utilisateur le sache.
        let mut store = AccountStore::new();
        store.add(account("a1", "Travail")).expect("premier ajout");

        let error = store
            .add(account("a1", "Autre"))
            .expect_err("doublon refusé");
        assert!(matches!(error, ConfigError::DuplicateAccount(_)));
        // Le compte d'origine est intact.
        assert_eq!(
            store
                .get(&AccountId::new("a1").expect("identifiant valide"))
                .expect("compte présent")
                .label,
            "Travail"
        );
    }

    #[test]
    fn upsert_replaces_without_error() {
        let mut store = AccountStore::new();
        store.add(account("a1", "Travail")).expect("ajout");
        store.upsert(account("a1", "Travail modifié"));

        assert_eq!(store.len(), 1);
        assert_eq!(
            store
                .get(&AccountId::new("a1").expect("identifiant valide"))
                .expect("compte présent")
                .label,
            "Travail modifié"
        );
    }

    #[test]
    fn remove_reports_unknown_ids() {
        let mut store = AccountStore::new();
        let unknown = AccountId::new("inexistant").expect("identifiant valide");
        assert!(matches!(
            store.remove(&unknown),
            Err(ConfigError::UnknownAccount(_))
        ));
    }

    #[test]
    fn all_returns_accounts_sorted_by_label() {
        // Sans tri, l'ordre d'un HashMap varie d'une exécution à l'autre et la
        // liste se réorganiserait à chaque démarrage de l'application.
        let mut store = AccountStore::new();
        store.add(account("z", "Zèbre")).expect("ajout");
        store.add(account("a", "Alpha")).expect("ajout");
        store.add(account("m", "Milieu")).expect("ajout");

        let labels: Vec<&str> = store.all().iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, vec!["Alpha", "Milieu", "Zèbre"]);
    }

    #[test]
    fn empty_store_reports_itself_empty() {
        let store = AccountStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.all().is_empty());
    }
}

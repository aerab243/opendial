// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Surface FFI des comptes SIP.
//!
//! Chaque fonction de ce module devient une fonction Dart après génération.
//! Aucune logique métier n'y apparaît : le travail est fait par
//! [`od_ffi::AccountService`], et ce module ne fait que traduire.

use std::sync::{Mutex, OnceLock};

use od_core::SignalingPort;
use od_ffi::accounts::{AccountDraft as DomainDraft, AccountService};
use od_sip::SipAgent;

/// Erreur franchissant la frontière FFI sous forme sérialisable.
///
/// `flutter_rust_bridge` ne peut pas transporter l'erreur du domaine telle
/// quelle : on la réduit à un message, que Dart affiche directement.
///
/// Le type généré côté Dart n'implémente pas `toString()` : interpoler
/// l'objet afficherait `Instance of 'FfiErrorInfo'` à l'utilisateur. Il faut
/// donc **toujours lire `.message`** dans l'interface — c'est la raison pour
/// laquelle le champ est public et unique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiErrorInfo {
    /// Message prêt à être affiché.
    pub message: String,
}

impl From<od_ffi::FfiError> for FfiErrorInfo {
    fn from(value: od_ffi::FfiError) -> Self {
        Self {
            message: value.message,
        }
    }
}

/// Compte tel que l'interface le reçoit.
///
/// Les champs sont en camelCase à dessein : `flutter_rust_bridge` les
/// transpose tels quels côté Dart, où c'est la convention. Les renommer en
/// snake_case obligerait l'interface à jongler entre deux styles.
#[allow(non_snake_case, reason = "convention Dart côté interface")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountView {
    /// Identifiant stable.
    pub id: String,
    /// Libellé affiché.
    pub label: String,
    /// Adresse d'enregistrement.
    pub addressOfRecord: String,
    /// Hôte du serveur.
    pub registrarHost: String,
    /// Port du serveur, ou 0 si le défaut du transport s'applique.
    pub registrarPort: u16,
    /// Transport : `"udp"`, `"tcp"` ou `"tls"`.
    pub transport: String,
    /// État : `"unregistered"`, `"registering"`, `"registered"` ou `"failed"`.
    pub status: String,
    /// Détail lisible, vide si rien à signaler.
    pub statusDetail: String,
}

impl From<od_ffi::AccountInfo> for AccountView {
    fn from(value: od_ffi::AccountInfo) -> Self {
        Self {
            id: value.id,
            label: value.label,
            addressOfRecord: value.address_of_record,
            registrarHost: value.registrar_host,
            registrarPort: value.registrar_port,
            transport: value.transport,
            status: value.status,
            statusDetail: value.status_detail,
        }
    }
}

/// Port local par défaut de l'agent SIP.
///
/// 0 laisse le système attribuer un port libre : c'est le comportement correct
/// pour un client, qui n'a pas besoin d'un port fixe puisqu'il s'enregistre
/// auprès d'un serveur et non l'inverse.
const DEFAULT_LOCAL_PORT: u16 = 0;

/// Service de comptes, conservé pour la durée de vie de l'application.
///
/// L'agent SIP contient un thread dédié : le recréer à chaque appel
/// interromprait l'enregistrement et rendrait le compte injoignable.
static SERVICE: OnceLock<Mutex<AccountService>> = OnceLock::new();

/// Renvoie le service global, en le démarrant au premier appel.
fn service() -> Result<&'static Mutex<AccountService>, FfiErrorInfo> {
    if let Some(service) = SERVICE.get() {
        return Ok(service);
    }

    let agent = SipAgent::start(DEFAULT_LOCAL_PORT).map_err(|error| FfiErrorInfo {
        message: format!("impossible de démarrer la signalisation SIP : {error}"),
    })?;

    let service = AccountService::new(Box::new(agent) as Box<dyn SignalingPort>);
    // `set` échoue si un autre thread a gagné la course ; dans ce cas on
    // réutilise l'instance déjà installée, ce qui évite deux agents.
    let _ = SERVICE.set(Mutex::new(service));
    SERVICE.get().ok_or_else(|| FfiErrorInfo {
        message: "initialisation du service impossible".to_owned(),
    })
}

/// Ajoute un compte et tente de l'enregistrer immédiatement.
///
/// # Erreurs
///
/// Renvoie un message lisible si les paramètres sont invalides ou si le compte
/// existe déjà. Un échec d'enregistrement n'est **pas** une erreur : il est
/// consigné dans le champ `status` du compte renvoyé.
pub fn add_account(
    label: String,
    username: String,
    password: String,
    server: String,
    transport: String,
) -> Result<AccountView, FfiErrorInfo> {
    let mut guard = service()?
        .lock()
        .map_err(|_| FfiErrorInfo {
            message: "le service de comptes est indisponible".to_owned(),
        })?;

    guard
        .add_account(DomainDraft {
            label,
            username,
            password,
            server,
            transport,
        })
        .map(AccountView::from)
        .map_err(FfiErrorInfo::from)
}

/// Renvoie la liste des comptes configurés.
pub fn list_accounts() -> Result<Vec<AccountView>, FfiErrorInfo> {
    let guard = service()?
        .lock()
        .map_err(|_| FfiErrorInfo {
            message: "le service de comptes est indisponible".to_owned(),
        })?;

    Ok(guard
        .list_accounts()
        .into_iter()
        .map(AccountView::from)
        .collect())
}

/// Relance l'enregistrement d'un compte existant.
#[allow(non_snake_case, reason = "convention Dart côté interface")]
pub fn register_account(accountId: String) -> Result<AccountView, FfiErrorInfo> {
    let mut guard = service()?
        .lock()
        .map_err(|_| FfiErrorInfo {
            message: "le service de comptes est indisponible".to_owned(),
        })?;

    guard
        .register_account(&accountId)
        .map(AccountView::from)
        .map_err(FfiErrorInfo::from)
}

/// Retire un compte et le désenregistre.
#[allow(non_snake_case, reason = "convention Dart côté interface")]
pub fn remove_account(accountId: String) -> Result<(), FfiErrorInfo> {
    let mut guard = service()?
        .lock()
        .map_err(|_| FfiErrorInfo {
            message: "le service de comptes est indisponible".to_owned(),
        })?;

    guard.remove_account(&accountId).map_err(FfiErrorInfo::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_view_maps_every_field() {
        let info = od_ffi::AccountInfo {
            id: "1001".to_owned(),
            label: "Travail".to_owned(),
            address_of_record: "1001@pbx".to_owned(),
            registrar_host: "pbx".to_owned(),
            registrar_port: 5060,
            transport: "udp".to_owned(),
            status: "registered".to_owned(),
            status_detail: "300 s".to_owned(),
        };

        let view = AccountView::from(info);
        assert_eq!(view.id, "1001");
        assert_eq!(view.registrarPort, 5060);
        assert_eq!(view.status, "registered");
        assert_eq!(view.addressOfRecord, "1001@pbx");
    }

    #[test]
    fn service_starts_once_and_is_reused() {
        // Deux appels successifs doivent renvoyer la MÊME instance : recréer
        // l'agent interromprait l'enregistrement en cours.
        let first = service().expect("premier accès");
        let second = service().expect("second accès");
        assert!(std::ptr::eq(first, second));
    }
}

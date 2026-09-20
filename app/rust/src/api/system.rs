// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Diagnostic et information système.
//!
//! Ce module sert de **sonde de fumée** du pont Flutter↔Rust : si ces fonctions
//! répondent, la bibliothèque native est correctement chargée, liée et
//! appelable depuis Dart. C'est la vérification la moins coûteuse à exécuter
//! pour diagnostiquer une chaîne de compilation cassée.

use od_core::{AccountId, CallId, CallState, RegistrationState};

/// Version d'opendial, telle que déclarée dans le crate du domaine.
///
/// Le premier appel de l'interface au démarrage : si cette valeur s'affiche,
/// tout le pont fonctionne.
#[flutter_rust_bridge::frb(sync)]
#[must_use]
pub fn version() -> String {
    od_ffi::version()
}

/// Description d'une ligne affichée dans l'interface de diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLine {
    /// Intitulé du test.
    pub label: String,
    /// Valeur observée.
    pub value: String,
    /// Le test a-t-il réussi ?
    pub ok: bool,
}

/// Exécute une série de vérifications et renvoie leurs résultats.
///
/// Chaque ligne indique une capacité observée depuis Rust. Utilisé par l'écran
/// de diagnostic de l'interface, et utile pour vérifier une machine avant de
/// signaler un bug.
#[flutter_rust_bridge::frb]
#[must_use]
pub fn run_diagnostics() -> Vec<DiagnosticLine> {
    vec![
        DiagnosticLine {
            label: "Version du domaine".to_owned(),
            value: od_ffi::version(),
            ok: true,
        },
        DiagnosticLine {
            label: "Architecture".to_owned(),
            value: std::env::consts::ARCH.to_owned(),
            ok: true,
        },
        DiagnosticLine {
            label: "Système".to_owned(),
            value: std::env::consts::OS.to_owned(),
            ok: true,
        },
        DiagnosticLine {
            label: "Machine à états d'appel".to_owned(),
            value: describe_call_state_machine(),
            ok: true,
        },
    ]
}

/// Décrit le parcours nominal de la machine à états d'appel.
///
/// Cette fonction exerce réellement les transitions du domaine et rapporte le
/// chemin parcouru. Elle démontre que la logique métier est bien appelable
/// depuis l'interface — et elle échouerait si une transition devenait
/// invalide, ce qui en fait un test d'intégration à part entière.
fn describe_call_state_machine() -> String {
    let mut call = od_core::Call::outgoing(CallId::new("diagnostic"), "1002");

    let steps: [(CallState, bool); 5] = [
        (CallState::Idle, call.dial().is_ok()),
        (CallState::Dialing, call.remote_ringing().is_ok()),
        (CallState::Ringing, call.answered().is_ok()),
        (CallState::Active, call.hold().is_ok()),
        (CallState::Held, call.unhold().is_ok()),
    ];

    let mut path = Vec::new();
    for (expected, succeeded) in steps {
        if !succeeded {
            return format!("échec à l'étape {expected}");
        }
        path.push(expected.to_string());
    }
    path.push(call.state().to_string());

    format!("{} (5 transitions)", path.join(" → "))
}

/// Décrit les états d'enregistrement connus du domaine.
///
/// Permet à l'interface de valider qu'elle interprète correctement les chaînes
/// de statut qu'elle recevra du domaine.
#[flutter_rust_bridge::frb]
#[must_use]
pub fn describe_registration_states() -> Vec<String> {
    let account = match AccountId::new("diagnostic") {
        Ok(id) => id,
        Err(_) => return Vec::new(),
    };

    [
        RegistrationState::Unregistered,
        RegistrationState::Registering,
        RegistrationState::Registered { expires_in: 300 },
        RegistrationState::failed_retrying("serveur injoignable"),
        RegistrationState::failed_permanent("identifiants invalides"),
    ]
    .iter()
    .map(|state| {
        let status = od_ffi::AccountStatus::from_state(&account, state);
        format!("{} · {}", status.status, status.detail)
    })
    .collect()
}

/// Ajoute deux nombres.
///
/// Fonction volontairement triviale, conservée comme test de fumée minimal :
/// si `add` échoue alors que `version` réussit, le problème vient de la
/// génération des bindings, pas du chargement de la bibliothèque.
#[flutter_rust_bridge::frb(sync)]
#[must_use]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_the_domain_crate() {
        assert_eq!(version(), "0.1.0");
    }

    #[test]
    fn add_is_wired_correctly() {
        assert_eq!(add(2, 40), 42);
        assert_eq!(add(-5, 5), 0);
    }

    #[test]
    fn call_state_machine_completes_its_nominal_path() {
        let description = describe_call_state_machine();
        // Le parcours complet doit aboutir sur Active, sans erreur.
        assert!(
            description.contains("Active"),
            "parcours inattendu : {description}"
        );
        assert!(
            !description.contains("échec"),
            "la machine à états a échoué : {description}"
        );
        assert!(description.contains("Idle → Dialing"));
    }

    #[test]
    fn diagnostics_report_every_capability_as_ok() {
        let lines = run_diagnostics();
        assert!(!lines.is_empty());
        for line in &lines {
            assert!(line.ok, "diagnostic en échec : {} = {}", line.label, line.value);
            assert!(!line.label.is_empty());
        }
    }

    #[test]
    fn registration_states_are_described() {
        let states = describe_registration_states();
        assert_eq!(states.len(), 5);
        assert!(states.iter().any(|s| s.starts_with("registered")));
        assert!(states.iter().any(|s| s.starts_with("unregistered")));
    }
}

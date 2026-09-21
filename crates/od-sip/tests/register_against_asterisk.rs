// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test d'intégration : enregistrement réel contre un serveur SIP.
//!
//! Ce test exige un serveur joignable — celui du banc de test :
//!
//! ```bash
//! docker compose -f testbed/docker-compose.yml up -d
//! cargo test -p od-sip --test register_against_asterisk -- --ignored
//! ```
//!
//! Il est marqué `#[ignore]` : il dépend d'un service externe, et l'inclure
//! dans `cargo test` rendrait la suite non déterministe. C'est un test de
//! **validation**, exécuté volontairement, pas à chaque exécution.
//!
//! ## Pourquoi il est indispensable
//!
//! Les tests unitaires de `mapping` vérifient la traduction des réponses, mais
//! s'appuient sur des valeurs fabriquées. Seul un vrai serveur peut confirmer
//! que l'endpoint s'ouvre, que le digest est correctement calculé, et que
//! rsipstack interprète la réponse comme nous l'attendons.

use std::net::TcpStream;
use std::time::Duration;

use od_core::{
    Account, AccountId, Credentials, Registrar, RegistrationState, SignalingPort, Transport,
};
use od_sip::SipAgent;

/// Port et hôte du banc de test.
const TESTBED_HOST: &str = "127.0.0.1";
const TESTBED_PORT: u16 = 5060;

/// Vérifie que le banc de test est joignable.
///
/// Sans ce contrôle, un serveur absent produirait un échec de test après
/// trente secondes d'attente, avec un message évoquant un problème de
/// signalisation plutôt qu'un conteneur éteint.
fn testbed_is_reachable() -> bool {
    TcpStream::connect_timeout(
        &format!("{TESTBED_HOST}:{TESTBED_PORT}")
            .parse()
            .expect("adresse valide"),
        Duration::from_secs(2),
    )
    .is_ok()
}

fn account(username: &str, password: &str) -> Account {
    Account::new(
        AccountId::new(format!("test-{username}")).expect("identifiant valide"),
        "Banc de test",
        format!("{username}@{TESTBED_HOST}"),
        Credentials::new(username, password).expect("identifiants valides"),
        Registrar::new(TESTBED_HOST)
            .expect("hôte valide")
            .with_port(TESTBED_PORT)
            .with_transport(Transport::Udp),
    )
    .expect("compte valide")
}

#[test]
#[ignore = "exige le banc de test Asterisk (docker compose up)"]
fn registers_successfully_with_valid_credentials() {
    assert!(
        testbed_is_reachable(),
        "banc de test injoignable sur {TESTBED_HOST}:{TESTBED_PORT} — \
         lancez `docker compose -f testbed/docker-compose.yml up -d`"
    );

    // Port local haut et libre : n'entre pas en conflit avec un agent réel.
    let mut agent = SipAgent::start(0).expect("démarrage de l'agent");
    let account = account("1001", "opendial");

    let state = agent
        .register(&account)
        .expect("l'enregistrement ne doit pas échouer au niveau transport");

    // Nettoyage systématique : Asterisk limite le nombre de contacts par
    // poste, et un contact laissé ferait échouer un test ultérieur.
    let _ = agent.unregister(&account.id);

    match &state {
        RegistrationState::Registered { expires_in } => {
            // Le serveur peut accorder moins que demandé ; l'essentiel est
            // qu'il accorde une durée exploitable.
            assert!(
                *expires_in > 0,
                "le serveur doit accorder une durée d'enregistrement"
            );
            println!("✓ enregistré, expire dans {expires_in} s");
        }
        other => panic!("enregistrement attendu, obtenu : {other}"),
    }
}

#[test]
#[ignore = "exige le banc de test Asterisk (docker compose up)"]
fn rejects_invalid_credentials_as_permanent() {
    assert!(
        testbed_is_reachable(),
        "banc de test injoignable — lancez `docker compose -f testbed/docker-compose.yml up -d`"
    );

    let mut agent = SipAgent::start(0).expect("démarrage de l'agent");
    let account = account("1001", "mauvais-mot-de-passe");

    let state = agent
        .register(&account)
        .expect("l'échec d'authentification n'est pas une erreur de transport");

    // Un enregistrement refusé ne crée pas de contact, mais on nettoie par
    // symétrie — un échec partiel laisserait un contact orphelin.
    let _ = agent.unregister(&account.id);

    match &state {
        RegistrationState::Failed { reason, retrying } => {
            // Point crucial : un mot de passe erroné ne doit PAS déclencher de
            // reprise automatique, sous peine de boucle infinie contre le
            // serveur et de verrouillage du compte.
            assert!(
                !retrying,
                "un mot de passe erroné ne doit pas être réessayé : {reason}"
            );
            println!("✓ refus correctement signalé comme définitif : {reason}");
        }
        other => panic!("échec attendu, obtenu : {other}"),
    }
}

#[test]
#[ignore = "exige le banc de test Asterisk (docker compose up)"]
fn unregister_actually_removes_the_contact_on_the_server() {
    assert!(
        testbed_is_reachable(),
        "banc de test injoignable — lancez `docker compose -f testbed/docker-compose.yml up -d`"
    );

    // Ce test couvre un défaut réellement observé : la désinscription se
    // contentait de retirer l'objet local, en laissant le contact sur le
    // serveur jusqu'à son expiration. Un utilisateur retirant son compte
    // l'aurait vu continuer à sonner.
    let mut agent = SipAgent::start(0).expect("démarrage de l'agent");
    let account = account("1001", "opendial");

    let state = agent.register(&account).expect("enregistrement");
    assert!(
        state.is_registered(),
        "prérequis : le compte doit s'enregistrer"
    );

    let before = contacts_for("1001");
    assert!(
        before > 0,
        "prérequis : le serveur doit connaître le contact"
    );

    agent.unregister(&account.id).expect("désinscription");

    // Le serveur a besoin d'un court instant pour traiter le REGISTER à
    // expiration nulle.
    std::thread::sleep(Duration::from_millis(500));

    let after = contacts_for("1001");
    // Si la vérification échoue, le contact est déjà retiré côté client : le
    // relancer ne le rétablirait pas, et le test suivant repartirait propre.
    assert!(
        after < before,
        "le contact doit disparaître du serveur : {before} avant, {after} après"
    );
}

/// Compte les contacts d'un utilisateur, tels que le serveur les voit.
///
/// Interroge Asterisk en ligne de commande : c'est la seule source fiable pour
/// vérifier ce que le serveur a réellement enregistré, par opposition à ce que
/// notre client croit.
///
/// Le chemin du fichier compose est construit depuis la racine du dépôt et non
/// depuis le répertoire courant : `cargo test` s'exécute dans le répertoire du
/// crate, où `testbed/` n'existe pas.
fn contacts_for(username: &str) -> usize {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let compose = format!("{manifest_dir}/../../testbed/docker-compose.yml");

    let output = std::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose,
            "exec",
            "-T",
            "asterisk",
            "asterisk",
            "-rx",
            "pjsip show contacts",
        ])
        .output()
        .expect("interrogation du serveur");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("Contact:") && line.contains(username))
        .count()
}

#[test]
#[ignore = "exige le banc de test Asterisk (docker compose up)"]
fn second_account_registers_independently() {
    assert!(
        testbed_is_reachable(),
        "banc de test injoignable — lancez `docker compose -f testbed/docker-compose.yml up -d`"
    );

    // Deux comptes distincts sur le même agent : vérifie que la table des
    // enregistrements ne mélange pas les identités.
    let mut agent = SipAgent::start(0).expect("démarrage de l'agent");

    let first_account = account("1001", "opendial");
    let second_account = account("1002", "opendial");

    let first = agent
        .register(&first_account)
        .expect("enregistrement du premier compte");
    let second = agent
        .register(&second_account)
        .expect("enregistrement du second compte");

    // Nettoyage avant les assertions : même en cas d'échec, on ne laisse pas
    // de contacts saturer le banc de test.
    let _ = agent.unregister(&first_account.id);
    let _ = agent.unregister(&second_account.id);

    assert!(first.is_registered(), "premier compte : {first}");
    assert!(second.is_registered(), "second compte : {second}");
    println!("✓ les deux comptes sont enregistrés indépendamment");
}

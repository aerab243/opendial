// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Tests d'intégration des comptes SIP, à travers le pont Flutter↔Rust.
///
/// Ces tests **appellent le vrai code Rust** : ils ne simulent rien. C'est la
/// vérification de bout en bout de la Phase 1 — validation des saisies,
/// enregistrement, gestion des erreurs.
///
/// Les tests qui exigent un serveur SIP réel sont marqués `skip` par défaut :
/// ils ne s'exécutent que si le banc de test est joignable, détecté au
/// démarrage. Cela garde `flutter test` déterministe tout en permettant la
/// validation complète en local.
library;

import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opendial/src/rust/api/accounts.dart';
import 'package:opendial/src/rust/frb_generated.dart';

/// Chemin de la bibliothèque native compilée par cargokit.
///
/// `flutter test` s'exécute hors du bundle de l'application : la recherche
/// automatique échoue, on indique donc où la trouver.
String? _locateNativeLibrary() {
  const candidates = [
    'build/linux/x64/debug/plugins/opendial_bridge/libopendial_bridge.so',
    'rust/target/debug/libopendial_bridge.so',
  ];
  for (final path in candidates) {
    if (File(path).existsSync()) return path;
  }
  return null;
}

/// Indique si le banc de test Asterisk répond.
Future<bool> _testbedIsReachable() async {
  try {
    final socket = await Socket.connect(
      '127.0.0.1',
      5060,
      timeout: const Duration(seconds: 2),
    );
    socket.destroy();
    return true;
  } on Object {
    return false;
  }
}

/// Extrait le message d'une erreur traversant la FFI.
///
/// Le type généré côté Dart n'implémente pas `toString()` : interpoller
/// l'objet donnerait `Instance of 'FfiErrorInfo'` au lieu du message. C'est un
/// piège à connaître, et l'interface doit lire `.message` de la même façon.
String _message(Object error) =>
    error is FfiErrorInfo ? error.message : '$error';

void main() {
  late bool testbedAvailable;

  setUpAll(() async {
    final libraryPath = _locateNativeLibrary();
    await OpendialBridge.init(
      externalLibrary:
          libraryPath == null ? null : ExternalLibrary.open(libraryPath),
    );
    testbedAvailable = await _testbedIsReachable();
  });

  // ---------------------------------------------------------------------
  // Validation des saisies — ne dépend d'aucun serveur
  // ---------------------------------------------------------------------

  group('Validation des saisies', () {
    test('un serveur vide est refusé avec un message explicite', () async {
      try {
        await addAccount(
          label: 'Test',
          username: '1001',
          password: 'x',
          server: '   ',
          transport: 'udp',
        );
        fail('un serveur vide doit être refusé');
      } catch (error) {
        expect(_message(error), contains('serveur'));
      }
    });

    test('un transport inconnu est refusé en indiquant les valeurs valides',
        () async {
      try {
        await addAccount(
          label: 'Test',
          username: '1001',
          password: 'x',
          server: 'pbx.exemple.com',
          transport: 'pigeon-voyageur',
        );
        fail('un transport inconnu doit être refusé');
      } catch (error) {
        // Le message doit indiquer les valeurs acceptées : « transport
        // inconnu » seul obligerait l'utilisateur à consulter la documentation.
        expect(_message(error), contains('udp'));
        expect(_message(error), contains('tcp'));
        expect(_message(error), contains('tls'));
      }
    });

    test('un utilisateur vide est refusé', () async {
      try {
        await addAccount(
          label: 'Test',
          username: '',
          password: 'x',
          server: 'pbx.exemple.com',
          transport: 'udp',
        );
        fail('un utilisateur vide doit être refusé');
      } catch (error) {
        expect(_message(error), isNotEmpty);
      }
    });

    test('un compte inconnu ne peut pas être enregistré', () async {
      try {
        await registerAccount(accountId: 'compte-qui-nexiste-pas');
        fail('un compte inconnu doit être refusé');
      } catch (error) {
        expect(_message(error), contains('compte-qui-nexiste-pas'));
      }
    });
  });

  // ---------------------------------------------------------------------
  // Enregistrement réel — exige le banc de test
  // ---------------------------------------------------------------------

  group('Enregistrement SIP', () {
    test('un compte valide s\'enregistre auprès du serveur', () async {
      if (!testbedAvailable) {
        markTestSkipped('banc de test injoignable sur 127.0.0.1:5060');
        return;
      }

      final account = await addAccount(
        label: 'Banc de test',
        username: '1001',
        password: 'opendial',
        server: '127.0.0.1:5060',
        transport: 'udp',
      );

      // C'est le critère de réussite de la Phase 1 : le compte est joignable.
      expect(account.status, 'registered');
      expect(account.registrarHost, '127.0.0.1');
      expect(account.registrarPort, 5060);
      expect(
        account.statusDetail,
        isNotEmpty,
        reason: 'le serveur doit accorder une durée d\'enregistrement',
      );

      // Nettoyage : on ne laisse pas de contact fantôme sur le serveur.
      await removeAccount(accountId: account.id);
    });

    test('des identifiants erronés sont signalés comme définitifs', () async {
      if (!testbedAvailable) {
        markTestSkipped('banc de test injoignable sur 127.0.0.1:5060');
        return;
      }

      final account = await addAccount(
        label: 'Mauvais mot de passe',
        username: '1002',
        password: 'ce-nest-pas-le-bon',
        server: '127.0.0.1:5060',
        transport: 'udp',
      );

      expect(account.status, 'failed');
      // Point crucial : pas de reprise automatique sur un mot de passe
      // erroné, sous peine de verrouiller le compte côté serveur.
      expect(
        account.statusDetail,
        isNot(contains('nouvelle tentative')),
        reason: 'un échec définitif ne doit pas annoncer de reprise',
      );

      await removeAccount(accountId: account.id);
    });

    test('le compte reste configuré même si le serveur est injoignable',
        () async {
      // Port fermé : la connexion échoue, mais la saisie ne doit pas être
      // perdue pour autant.
      final account = await addAccount(
        label: 'Serveur absent',
        username: '9999',
        password: 'x',
        server: '127.0.0.1:9',
        transport: 'udp',
      );

      expect(account.status, 'failed');

      // Le compte existe toujours : l'utilisateur pourra corriger l'adresse
      // ou réessayer plus tard.
      final accounts = await listAccounts();
      expect(accounts.any((a) => a.id == '9999'), isTrue);

      await removeAccount(accountId: account.id);
    });

    test('la liste reflète les comptes ajoutés', () async {
      if (!testbedAvailable) {
        markTestSkipped('banc de test injoignable sur 127.0.0.1:5060');
        return;
      }

      await addAccount(
        label: 'Un',
        username: '1001',
        password: 'opendial',
        server: '127.0.0.1:5060',
        transport: 'udp',
      );
      await addAccount(
        label: 'Deux',
        username: '1002',
        password: 'opendial',
        server: '127.0.0.1:5060',
        transport: 'udp',
      );

      final accounts = await listAccounts();
      expect(accounts.length, greaterThanOrEqualTo(2));

      await removeAccount(accountId: '1001');
      await removeAccount(accountId: '1002');

      final after = await listAccounts();
      expect(after.any((a) => a.id == '1001'), isFalse);
      expect(after.any((a) => a.id == '1002'), isFalse);
    });
  });
}

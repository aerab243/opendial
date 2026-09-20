// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Tests d'intégration du pont Flutter↔Rust.
///
/// Ces tests **chargent réellement la bibliothèque native** et appellent le
/// domaine Rust : ils ne simulent rien. C'est la vérification de bout en bout
/// de la chaîne complète — compilation du crate, liaison par cargokit,
/// génération des bindings, sérialisation des appels.
///
/// Un test unitaire Dart ne pourrait pas les remplacer : il passerait sans
/// qu'aucune ligne de Rust ne soit exécutée.
library;

import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opendial/src/rust/api/system.dart';
import 'package:opendial/src/rust/frb_generated.dart';

/// Chemin de la bibliothèque native compilée par cargokit.
///
/// `flutter test` s'exécute dans le répertoire du paquet, hors du bundle de
/// l'application : la recherche automatique de la bibliothèque échoue. On
/// indique donc explicitement où elle se trouve.
///
/// Le chemin correspond à la sortie de cargokit, utilisée par le plugin
/// `rust_builder`. Il existe après un `flutter build linux`.
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

void main() {
  // Le chargement de la bibliothèque native est indispensable avant tout
  // appel : il est fait une seule fois pour l'ensemble des tests.
  setUpAll(() async {
    final libraryPath = _locateNativeLibrary();
    await OpendialBridge.init(
      externalLibrary: libraryPath == null
          ? null
          : ExternalLibrary.open(libraryPath),
    );
  });

  group('Sonde de fumée', () {
    test('la version du domaine est accessible depuis Dart', () {
      // Si ce test passe, la bibliothèque est chargée et un appel synchrone
      // traverse la frontière FFI.
      expect(version(), '0.1.0');
    });

    test('un appel synchrone avec arguments et retour fonctionne', () {
      expect(add(a: 20, b: 22), 42);
      expect(add(a: -5, b: 5), 0);
    });
  });

  group('Types complexes à travers la frontière', () {
    test('une liste de structures est correctement sérialisée', () async {
      final diagnostics = await runDiagnostics();

      expect(diagnostics, isNotEmpty);
      // Chaque ligne doit être complète : un champ oublié à la sérialisation
      // se traduirait par une chaîne vide ou un échec.
      for (final line in diagnostics) {
        expect(line.label, isNotEmpty);
        expect(line.value, isNotEmpty);
        expect(line.ok, isTrue, reason: '${line.label} : ${line.value}');
      }
    });

    test('une liste de chaînes est correctement transmise', () async {
      final states = await describeRegistrationStates();

      expect(states, hasLength(5));
      expect(states.any((s) => s.startsWith('registered')), isTrue);
      expect(states.any((s) => s.startsWith('unregistered')), isTrue);
    });
  });

  group('Logique métier exécutée depuis Dart', () {
    test('la machine à états d\'appel parcourt son cycle complet', () async {
      // C'est le test le plus significatif : la valeur affichée provient de
      // l'exécution réelle de cinq transitions dans od-core, à travers la
      // frontière FFI. Elle échouerait si une transition devenait invalide.
      final diagnostics = await runDiagnostics();
      final callState = diagnostics.firstWhere(
        (line) => line.label.contains('Machine'),
      );

      expect(
        callState.value,
        contains('Idle'),
        reason: 'le parcours doit démarrer à Idle',
      );
      expect(
        callState.value,
        contains('Active'),
        reason: 'le parcours doit aboutir à Active',
      );
      expect(
        callState.value,
        isNot(contains('échec')),
        reason: 'aucune transition ne doit échouer',
      );
    });
  });
}

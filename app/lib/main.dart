// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Point d'entrée de l'application opendial.
///
/// Phase 1 : l'écran principal gère les comptes SIP. L'écran de diagnostic
/// reste accessible depuis la barre d'application — il sert à vérifier le bon
/// fonctionnement du pont Flutter↔Rust, et à recueillir des informations
/// exploitables en cas de problème sur une machine inconnue.
library;

import 'package:flutter/material.dart';

import 'features/accounts/accounts_screen.dart';
import 'features/diagnostic/diagnostic_screen.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  // L'initialisation du pont doit précéder tout appel à l'API Rust : elle
  // localise et charge la bibliothèque native, et enregistre le protocole de
  // communication. Appeler une fonction Rust avant cette étape échoue à
  // l'exécution, avec un message peu explicite.
  //
  // `OpendialBridge` est la classe racine définie par `dart_entrypoint_class_name`
  // dans flutter_rust_bridge.yaml ; elle coïncide avec le nom de notre API.
  await OpendialBridge.init();
  runApp(const OpendialApp());
}

/// Application opendial.
class OpendialApp extends StatelessWidget {
  /// Crée l'application.
  const OpendialApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'opendial',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF2E7D5B)),
        useMaterial3: true,
      ),
      home: const HomeScreen(),
    );
  }
}

/// Écran principal.
///
/// En Phase 1, il se limite à la gestion des comptes. Le composeur, l'écran
/// d'appel, les contacts et l'historique arrivent aux phases suivantes.
class HomeScreen extends StatelessWidget {
  /// Crée l'écran principal.
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('opendial'),
        actions: [
          IconButton(
            icon: const Icon(Icons.monitor_heart_outlined),
            tooltip: 'Diagnostic',
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => const DiagnosticScreen(),
              ),
            ),
          ),
        ],
      ),
      body: const AccountsScreen(),
    );
  }
}

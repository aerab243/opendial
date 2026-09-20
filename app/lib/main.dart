// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Point d'entrée de l'application opendial.
///
/// En Phase 0, l'interface se limite à un écran de diagnostic : il prouve que
/// le pont Flutter↔Rust fonctionne de bout en bout. Les écrans métier
/// (comptes, composeur, appels) arrivent aux phases suivantes.
library;

import 'package:flutter/material.dart';

import 'src/rust/api/system.dart';
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
      home: const DiagnosticScreen(),
    );
  }
}

/// Écran de diagnostic du pont Flutter↔Rust.
///
/// Exerce réellement le pont : les valeurs affichées proviennent toutes du
/// domaine Rust, y compris le résultat de cinq transitions de la machine à
/// états d'appel. Si cet écran s'affiche, alors le chargement de la
/// bibliothèque, la génération des bindings et la logique métier fonctionnent.
class DiagnosticScreen extends StatefulWidget {
  /// Crée l'écran de diagnostic.
  const DiagnosticScreen({super.key});

  @override
  State<DiagnosticScreen> createState() => _DiagnosticScreenState();
}

class _DiagnosticScreenState extends State<DiagnosticScreen> {
  /// Résultat de la sonde de fumée synchrone.
  ///
  /// Calculé immédiatement : si le pont est cassé, l'erreur apparaît au
  /// premier rendu plutôt que de laisser un écran vide.
  late final String _version = version();

  /// Résultat de la vérification synchrone minimale.
  late final int _smokeTest = add(a: 20, b: 22);

  Future<List<DiagnosticLine>>? _diagnostics;
  Future<List<String>>? _registrationStates;

  @override
  void initState() {
    super.initState();
    _runChecks();
  }

  void _runChecks() {
    setState(() {
      _diagnostics = runDiagnostics();
      _registrationStates = describeRegistrationStates();
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('opendial'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Relancer les vérifications',
            onPressed: _runChecks,
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          _SectionCard(
            title: 'Sonde de fumée',
            subtitle: 'Appels synchrones au domaine Rust',
            children: [
              _ResultTile(
                label: 'version du domaine',
                value: _version,
                ok: true,
              ),
              _ResultTile(
                label: 'add(20, 22)',
                value: '$_smokeTest',
                ok: _smokeTest == 42,
              ),
            ],
          ),
          const SizedBox(height: 16),
          _AsyncSection(
            title: 'Diagnostic du domaine',
            future: _diagnostics,
            builder: (lines) => lines
                .map(
                  (line) => _ResultTile(
                    label: line.label,
                    value: line.value,
                    ok: line.ok,
                  ),
                )
                .toList(),
          ),
          const SizedBox(height: 16),
          _AsyncSection(
            title: 'États d\'enregistrement',
            subtitle: 'Chaînes produites par od-ffi pour l\'interface',
            future: _registrationStates,
            builder: (states) => states
                .map(
                  (state) => _ResultTile(
                    label: 'état',
                    value: state,
                    ok: true,
                  ),
                )
                .toList(),
          ),
          const SizedBox(height: 24),
          const _Footer(),
        ],
      ),
    );
  }
}

/// Carte regroupant une section de résultats.
class _SectionCard extends StatelessWidget {
  const _SectionCard({
    required this.title,
    required this.children,
    this.subtitle,
  });

  final String title;
  final String? subtitle;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: theme.textTheme.titleMedium),
            if (subtitle != null) ...[
              const SizedBox(height: 2),
              Text(
                subtitle!,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
            const Divider(height: 20),
            ...children,
          ],
        ),
      ),
    );
  }
}

/// Section dont le contenu dépend d'un appel Rust asynchrone.
class _AsyncSection<T> extends StatelessWidget {
  const _AsyncSection({
    required this.title,
    required this.future,
    required this.builder,
    this.subtitle,
  });

  final String title;
  final String? subtitle;
  final Future<List<T>>? future;
  final List<Widget> Function(List<T>) builder;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<T>>(
      future: future,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return _SectionCard(
            title: title,
            subtitle: subtitle,
            children: [
              _ResultTile(
                label: 'erreur',
                value: '${snapshot.error}',
                ok: false,
              ),
            ],
          );
        }
        if (!snapshot.hasData) {
          return _SectionCard(
            title: title,
            subtitle: subtitle,
            children: const [
              Padding(
                padding: EdgeInsets.symmetric(vertical: 8),
                child: LinearProgressIndicator(),
              ),
            ],
          );
        }
        return _SectionCard(
          title: title,
          subtitle: subtitle,
          children: builder(snapshot.data!),
        );
      },
    );
  }
}

/// Ligne de résultat avec indicateur visuel.
class _ResultTile extends StatelessWidget {
  const _ResultTile({
    required this.label,
    required this.value,
    required this.ok,
  });

  final String label;
  final String value;
  final bool ok;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            ok ? Icons.check_circle : Icons.error,
            size: 18,
            color: ok ? Colors.green : theme.colorScheme.error,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(label, style: theme.textTheme.labelMedium),
                SelectableText(
                  value,
                  style: theme.textTheme.bodySmall?.copyWith(
                    fontFamily: 'monospace',
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// Pied de page rappelant l'état du projet.
class _Footer extends StatelessWidget {
  const _Footer();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Text(
        'Phase 0 — fondations\n'
        'Le pont Flutter↔Rust est opérationnel.',
        textAlign: TextAlign.center,
        style: theme.textTheme.bodySmall?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

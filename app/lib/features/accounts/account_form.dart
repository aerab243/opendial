// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Formulaire d'ajout d'un compte SIP.
///
/// Le formulaire **ne valide presque rien** : il vérifie seulement que les
/// champs ne sont pas vides, pour éviter un aller-retour inutile vers Rust.
/// Toute la validation réelle — adresse de serveur, transport, cohérence — vit
/// dans `od-core`, où elle est testable sans interface. Dupliquer ces règles
/// ici créerait deux sources de vérité qui divergeraient.
library;

import 'package:flutter/material.dart';

import '../../src/rust/api/accounts.dart';

/// Formulaire de création de compte.
class AccountFormScreen extends StatefulWidget {
  /// Crée l'écran.
  const AccountFormScreen({super.key});

  @override
  State<AccountFormScreen> createState() => _AccountFormScreenState();
}

class _AccountFormScreenState extends State<AccountFormScreen> {
  final _formKey = GlobalKey<FormState>();
  final _labelController = TextEditingController();
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  final _serverController = TextEditingController(text: '127.0.0.1:5060');

  String _transport = 'udp';
  bool _submitting = false;
  String? _error;

  @override
  void dispose() {
    _labelController.dispose();
    _usernameController.dispose();
    _passwordController.dispose();
    _serverController.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (!(_formKey.currentState?.validate() ?? false)) return;

    setState(() {
      _submitting = true;
      _error = null;
    });

    try {
      await addAccount(
        label: _labelController.text.trim(),
        username: _usernameController.text.trim(),
        password: _passwordController.text,
        server: _serverController.text.trim(),
        transport: _transport,
      );
      if (!mounted) return;
      // `true` indique à la liste qu'un compte a été créé et qu'elle doit se
      // rafraîchir.
      Navigator.of(context).pop(true);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        // L'erreur vient du domaine, déjà formulée pour être affichée.
        // Le type généré n'implémente pas `toString()` : lire `.message` est
        // indispensable, sinon l'utilisateur verrait « Instance of ... ».
        _error = error is FfiErrorInfo ? error.message : error.toString();
        _submitting = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(title: const Text('Nouveau compte')),
      body: Form(
        key: _formKey,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            TextFormField(
              controller: _labelController,
              decoration: const InputDecoration(
                labelText: 'Nom du compte',
                hintText: 'Travail',
                helperText: 'Nom affiché dans la liste',
                border: OutlineInputBorder(),
              ),
              textInputAction: TextInputAction.next,
              validator: _required('Un nom est nécessaire pour identifier le compte'),
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _usernameController,
              decoration: const InputDecoration(
                labelText: 'Utilisateur SIP',
                hintText: '1001',
                helperText: 'Numéro de poste ou identifiant fourni par l\'opérateur',
                border: OutlineInputBorder(),
              ),
              textInputAction: TextInputAction.next,
              validator: _required('L\'utilisateur est requis'),
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _passwordController,
              decoration: const InputDecoration(
                labelText: 'Mot de passe',
                border: OutlineInputBorder(),
              ),
              obscureText: true,
              textInputAction: TextInputAction.next,
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _serverController,
              decoration: const InputDecoration(
                labelText: 'Serveur',
                hintText: 'pbx.exemple.com ou 192.168.1.10:5060',
                helperText: 'Le port est facultatif',
                border: OutlineInputBorder(),
              ),
              textInputAction: TextInputAction.next,
              validator: _required('L\'adresse du serveur est requise'),
            ),
            const SizedBox(height: 16),
            DropdownButtonFormField<String>(
              initialValue: _transport,
              decoration: const InputDecoration(
                labelText: 'Transport',
                helperText: 'TLS chiffre la signalisation — à préférer si le serveur le supporte',
                border: OutlineInputBorder(),
              ),
              items: const [
                DropdownMenuItem(value: 'udp', child: Text('UDP — défaut')),
                DropdownMenuItem(value: 'tcp', child: Text('TCP')),
                DropdownMenuItem(value: 'tls', child: Text('TLS — chiffré')),
              ],
              onChanged: (value) {
                if (value != null) setState(() => _transport = value);
              },
            ),
            if (_error != null) ...[
              const SizedBox(height: 20),
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: theme.colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(
                      Icons.error_outline,
                      color: theme.colorScheme.onErrorContainer,
                      size: 20,
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        _error!,
                        style: TextStyle(
                          color: theme.colorScheme.onErrorContainer,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
            const SizedBox(height: 24),
            FilledButton.icon(
              onPressed: _submitting ? null : _submit,
              icon: _submitting
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.save),
              label: Text(_submitting ? 'Enregistrement…' : 'Ajouter le compte'),
            ),
            const SizedBox(height: 12),
            Text(
              'Le compte sera enregistré immédiatement après l\'ajout. Si le '
              'serveur le refuse, vous verrez pourquoi dans la liste.',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }

  /// Validateur minimal : refuse les champs vides uniquement.
  ///
  /// Toute règle plus précise appartient au domaine — voir la note en tête de
  /// fichier.
  FormFieldValidator<String> _required(String message) {
    return (value) =>
        (value == null || value.trim().isEmpty) ? message : null;
  }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Écran de gestion des comptes SIP.
///
/// Objectif de la Phase 1 : configurer un compte, l'enregistrer sur un serveur
/// réel, et voir son état. Aucun audio n'est encore en jeu.
///
/// L'écran ne contient **aucune logique métier** : il appelle l'API générée et
/// affiche ce qu'elle renvoie. Toute la validation vit dans `od-core`.
library;

import 'package:flutter/material.dart';

import '../../src/rust/api/accounts.dart';
import 'account_form.dart';
import 'status_badge.dart';

/// Extrait le message lisible d'une erreur venue de Rust.
///
/// Le type généré par `flutter_rust_bridge` n'implémente pas `toString()` :
/// interpoller l'objet directement afficherait « Instance of 'FfiErrorInfo' »
/// au lieu du message. Toute erreur traversant la FFI doit donc passer par ici.
String _readableError(Object error) =>
    error is FfiErrorInfo ? error.message : '$error';

/// Écran listant les comptes SIP configurés.
class AccountsScreen extends StatefulWidget {
  /// Crée l'écran.
  const AccountsScreen({super.key});

  @override
  State<AccountsScreen> createState() => _AccountsScreenState();
}

class _AccountsScreenState extends State<AccountsScreen> {
  List<AccountView> _accounts = const [];
  String? _error;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  Future<void> _refresh() async {
    setState(() {
      _loading = true;
      _error = null;
    });

    try {
      final accounts = await listAccounts();
      if (!mounted) return;
      setState(() {
        _accounts = accounts;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = _readableError(error);
        _loading = false;
      });
    }
  }

  /// Ouvre le formulaire d'ajout et rafraîchit la liste en cas de succès.
  Future<void> _openAddForm() async {
    final created = await Navigator.of(context).push<bool>(
      MaterialPageRoute<bool>(builder: (_) => const AccountFormScreen()),
    );
    if (created ?? false) {
      await _refresh();
    }
  }

  /// Relance l'enregistrement d'un compte.
  Future<void> _reRegister(String accountId) async {
    try {
      await registerAccount(accountId: accountId);
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Échec : ${_readableError(error)}')),
      );
    }
    await _refresh();
  }

  /// Retire un compte, après confirmation.
  Future<void> _remove(AccountView account) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Retirer ce compte ?'),
        content: Text(
          'Le compte « ${account.label} » sera désenregistré et supprimé.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Annuler'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Retirer'),
          ),
        ],
      ),
    );

    if (confirmed ?? false) {
      try {
        await removeAccount(accountId: account.id);
      } catch (error) {
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Échec : ${_readableError(error)}')),
        );
      }
      await _refresh();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      // Pas d'AppBar : cet écran est imbriqué dans celui de l'accueil, qui
      // porte déjà la sienne. En empiler deux donnerait une double barre.
      body: _buildBody(),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _openAddForm,
        icon: const Icon(Icons.add),
        label: const Text('Ajouter'),
      ),
    );
  }

  Widget _buildBody() {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }

    if (_error != null) {
      return _Message(
        icon: Icons.error_outline,
        title: 'Impossible de charger les comptes',
        detail: _error!,
        action: FilledButton(
          onPressed: _refresh,
          child: const Text('Réessayer'),
        ),
      );
    }

    if (_accounts.isEmpty) {
      return const _Message(
        icon: Icons.person_add_alt,
        title: 'Aucun compte configuré',
        detail:
            'Ajoutez un compte SIP pour pouvoir passer et recevoir des appels.',
      );
    }

    return RefreshIndicator(
      onRefresh: _refresh,
      child: ListView.separated(
        padding: const EdgeInsets.only(bottom: 88),
        itemCount: _accounts.length,
        separatorBuilder: (_, _) => const Divider(height: 1),
        itemBuilder: (context, index) {
          final account = _accounts[index];
          return _AccountTile(
            account: account,
            onReRegister: () => _reRegister(account.id),
            onRemove: () => _remove(account),
          );
        },
      ),
    );
  }
}

/// Ligne représentant un compte.
class _AccountTile extends StatelessWidget {
  const _AccountTile({
    required this.account,
    required this.onReRegister,
    required this.onRemove,
  });

  final AccountView account;
  final VoidCallback onReRegister;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isFailed = account.status == 'failed';

    return ListTile(
      leading: StatusBadge(status: account.status),
      title: Text(account.label),
      subtitle: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            account.addressOfRecord,
            style: theme.textTheme.bodySmall,
          ),
          Text(
            '${account.registrarHost}'
            '${account.registrarPort == 0 ? '' : ':${account.registrarPort}'}'
            ' · ${account.transport}',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
          if (account.statusDetail.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text(
                account.statusDetail,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: isFailed
                      ? theme.colorScheme.error
                      : theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ),
        ],
      ),
      isThreeLine: true,
      trailing: PopupMenuButton<String>(
        onSelected: (value) {
          switch (value) {
            case 'register':
              onReRegister();
            case 'remove':
              onRemove();
          }
        },
        itemBuilder: (context) => [
          // Seul un compte en échec a besoin d'un nouvel essai manuel : dans
          // les autres cas, le rafraîchissement est automatique et un bouton
          // n'apporterait que de la confusion.
          if (isFailed)
            const PopupMenuItem(
              value: 'register',
              child: Text('Réessayer l\'enregistrement'),
            ),
          const PopupMenuItem(
            value: 'remove',
            child: Text('Retirer'),
          ),
        ],
      ),
    );
  }
}

/// Message centré, avec action facultative.
class _Message extends StatelessWidget {
  const _Message({
    required this.icon,
    required this.title,
    required this.detail,
    this.action,
  });

  final IconData icon;
  final String title;
  final String detail;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 48, color: theme.colorScheme.onSurfaceVariant),
            const SizedBox(height: 16),
            Text(title, style: theme.textTheme.titleMedium),
            const SizedBox(height: 8),
            Text(
              detail,
              textAlign: TextAlign.center,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            if (action != null) ...[
              const SizedBox(height: 24),
              action!,
            ],
          ],
        ),
      ),
    );
  }
}

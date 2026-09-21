// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Badge d'état d'un compte.
///
/// Le domaine produit des états sous forme de chaînes stables
/// (`registered`, `registering`, `failed`, `unregistered`). Ce widget les
/// traduit en icônes et couleurs : c'est de la **présentation**, aucune
/// décision n'est prise ici.
library;

import 'package:flutter/material.dart';

/// Badge circulaire reflétant l'état d'un compte.
class StatusBadge extends StatelessWidget {
  /// Crée le badge pour l'état donné.
  const StatusBadge({required this.status, super.key});

  /// État produit par le domaine.
  final String status;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;

    final (IconData icon, Color color, String label) = switch (status) {
      // Joignable : appels entrants et sortants possibles.
      'registered' => (
          Icons.check_circle,
          Colors.green.shade600,
          'Enregistré',
        ),
      // Tentative en cours : l'utilisateur doit patienter.
      'registering' => (
          Icons.sync,
          scheme.primary,
          'Enregistrement en cours',
        ),
      // Échec : l'utilisateur doit agir — vérifier les identifiants ou le
      // serveur.
      'failed' => (Icons.error, scheme.error, 'Échec'),
      // Jamais enregistré : état normal au moment de la création.
      _ => (Icons.circle_outlined, scheme.onSurfaceVariant, 'Non enregistré'),
    };

    return Tooltip(
      message: label,
      child: CircleAvatar(
        backgroundColor: color.withValues(alpha: 0.12),
        child: Icon(icon, color: color, size: 22),
      ),
    );
  }
}

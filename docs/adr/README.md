# Décisions d'architecture (ADR)

Ce dossier consigne les décisions structurantes d'opendial. Un ADR explique **pourquoi** un choix a été fait, dans quel contexte, et avec quelles conséquences assumées. Le code dit *comment* ; l'ADR dit *pourquoi*.

## Pourquoi ces documents

Les décisions les plus coûteuses d'un projet sont celles qu'on ne peut plus défaire : choix de licence, de stack, de frontières entre modules. Six mois plus tard, personne ne se souvient des raisons. Un ADR évite de re-débattre, et évite surtout qu'un nouveau contributeur « corrige » une décision qui était délibérée.

## Index

| N° | Titre | Statut |
|---|---|---|
| [0001](0001-why-not-microsip.md) | Réécriture propre plutôt que réutilisation de MicroSIP | Accepté |
| [0003](0003-licence-et-politique-dependances.md) | Licence MPL-2.0 et politique de dépendances | Accepté |
| 0002 | Découpage ports & adapters | À écrire |

## Statuts

- **Proposé** — en discussion, pas encore appliqué.
- **Accepté** — décision en vigueur, à respecter.
- **Déprécié** — plus applicable, conservé pour l'historique.
- **Remplacé par ADR-XXXX** — une décision ultérieure a pris le relais.

Un ADR accepté ne se modifie pas. Pour changer d'avis, on écrit un nouvel ADR qui remplace l'ancien — l'historique du raisonnement reste lisible.

## Modèle

```markdown
# ADR-NNNN — Titre à l'infinitif

- **Date** : AAAA-MM-JJ
- **Statut** : Proposé
- **Décideurs** : ...

## Contexte

Quel problème se pose, quelles contraintes s'appliquent. Les faits, pas les opinions.

## Décision

Ce qu'on décide, à l'indicatif et sans ambiguïté.

## Conséquences

### Positives
### Négatives — assumées

Ce qu'on perd, ce qu'on accepte de subir. Un ADR sans conséquences négatives est un ADR mal écrit.

## Références
```

## Conventions

- **Numérotation** : séquentielle, jamais réutilisée.
- **Langue** : français pour l'instant. Une migration vers l'anglais reste possible tant que le projet n'a pas de contributeurs non francophones — la décision sera tracée dans un ADR dédié.
- **Un ADR par décision.** Si un document en contient deux, le scinder.

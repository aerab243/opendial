# ADR-0003 — Licence MPL-2.0 et politique de dépendances

- **Date** : 2026-09-20
- **Statut** : Accepté
- **Décideurs** : mainteneurs d'opendial

## Contexte

L'ADR-0001 a écarté toute réutilisation du code de MicroSIP, ce qui libère le choix de licence. Ce choix est **structurant et difficilement réversible** : il détermine qui pourra utiliser opendial, sous quelles conditions, et quelles dépendances nous pourrons employer.

Le projet repose sur un écosystème de bibliothèques tierces (SIP, média, audio, codecs). Contrairement à MicroSIP, qui s'est soudé à PJSIP — **GPLv2 ou licence commerciale** — et en a hérité la licence, opendial doit choisir ses dépendances en connaissance de cause et se prémunir contre les contaminations accidentelles.

Un premier audit a déjà révélé un cas concret de métadonnée trompeuse (voir « Le cas spandsp »), ce qui justifie de formaliser la politique dès maintenant plutôt que de la découvrir plus tard.

## Décision 1 — opendial est distribué sous MPL-2.0

**Mozilla Public License 2.0.**

### Pourquoi

| Critère | MPL-2.0 |
|---|---|
| Copyleft | **Par fichier** — les modifications des fichiers MPL doivent rester ouvertes |
| Intégration propriétaire | **Permise** — un tiers peut lier opendial à un produit fermé |
| Double licence | **Possible** — l'auteur peut vendre une licence commerciale |
| Compatibilité dépendances | Excellente avec MIT, Apache-2.0, BSD, ISC |
| Compatibilité amont | Compatible GPLv2+, LGPLv2.1+ et AGPLv3 via la clause *Secondary License* (§3.3) |

**Clause §3.3 — et la décision qui l'accompagne.** La MPL-2.0 définit comme *Secondary Licenses* la GPLv2, la LGPLv2.1 et l'AGPLv3, « ou toute version ultérieure de celles-ci » — ce qui inclut donc la GPLv3 et la LGPLv3. Cette clause autorise un projet GPL à intégrer des fichiers opendial dans une œuvre plus vaste.

> ⚠️ **Cette compatibilité est conditionnelle, et nous devons la préserver activement.** Elle ne s'applique que si le logiciel n'est **pas** marqué « Incompatible With Secondary Licenses ». Ce marquage est un **choix explicite** de l'auteur, matérialisé par l'avis *Exhibit B* — il n'est pas appliqué par défaut.

**Décision : nous n'attacherons jamais l'avis Exhibit B à opendial.** L'en-tête de licence de chaque fichier utilisera l'*Exhibit A* standard, sans mention d'incompatibilité. Cela garantit qu'opendial reste intégrable dans un projet GPL/LGPL/AGPL — condition non négociable pour un logiciel de communication, dont l'écosystème est majoritairement copyleft (Linphone, Jami, Asterisk sont GPL).

**Le raisonnement.** MPL-2.0 offre le meilleur équilibre pour un softphone :

- Le copyleft par fichier garantit que **les améliorations au cœur d'opendial restent ouvertes** — la communauté en bénéficie, et nous aussi.
- Mais il **ne contamine pas** l'application qui l'intègre. Une entreprise peut embarquer opendial dans un client propriétaire, ce qui évite de s'aliéner les intégrateurs (l'erreur de la GPL, qui a poussé Linphone vers un modèle de double licence).
- Il **laisse la porte ouverte au double licence** à la Linphone : si un acteur veut du code fermé, nous pouvons vendre une exception. Ce serait **impossible en GPL**.

### Alternatives écartées

| Licence | Pourquoi écartée |
|---|---|
| **Apache-2.0** | Excellente (permissive + clause brevets), mais n'oblige pas à reverser les améliorations. Un concurrent pourrait améliorer opendial et tout garder pour lui. |
| **GPLv3** | Copyleft fort, garantit que le projet ne deviendra jamais propriétaire. Mais interdit toute intégration fermée et tout double licence — décision définitive. |
| **AGPLv3** | Pertinente pour un service hébergé, sans objet pour un logiciel desktop. |
| **MIT** | Maximale adoption, mais aucune protection : n'importe qui peut fermer une version dérivée. |

## Décision 2 — Politique de dépendances

### Règle générale

> **Aucune dépendance n'entre dans opendial sans que sa licence — et celle de son amont — soit vérifiée.**

La déclaration de licence d'un crate Rust **n'est pas une preuve**. Elle est renseignée par l'auteur du crate, sans vérification. Le cas spandsp ci-dessous démontre que l'écart entre la métadonnée et la réalité peut être significatif.

### Liste blanche — utilisation libre

Ces licences sont permissives, compatibles avec MPL-2.0, et n'imposent aucune obligation à opendial :

`MIT` · `Apache-2.0` · `BSD-2-Clause` · `BSD-3-Clause` · `ISC` · `Zlib` · `Unicode-DFS-2016` · `CC0-1.0` · `MPL-2.0`

### Liste grise — audit obligatoire avant utilisation

| Licence | Condition |
|---|---|
| **LGPL-2.1 / LGPL-3.0** | Acceptable **uniquement** en liaison dynamique, avec documentation de l'audit et fourniture du code source de la partie LGPL. Jamais en liaison statique. |
| **Apache-2.0 avec clauses additionnelles** | Vérifier les clauses (brevets, attribution, export). |
| **Licence non déclarée** | Traiter comme interdite jusqu'à clarification auprès de l'auteur. |

### Liste noire — interdites

| Licence | Raison |
|---|---|
| **GPL-2.0 / GPL-3.0** | Contaminerait opendial et forcerait la GPL. C'est exactement ce que l'ADR-0001 évite. |
| **AGPL-3.0** | Contamination plus large encore (déclenchement sur usage réseau), incompatible avec la distribution desktop. |
| **Licences propriétaires ou de recherche** | Interdisent la redistribution. |

### Le cas spandsp — pourquoi cette politique existe

`rustrtc` déclare une dépendance optionnelle :

```toml
spandsp-sys = { version = "0.1.5", optional = true }

[features]
default = []
t38 = ["dep:spandsp-sys"]
```

**Situation :**

- La métadonnée du crate Rust `spandsp-sys` déclare **MIT**.
- Le code C amont de **spandsp** (Steve Underwood, désormais maintenu par FreeSWITCH) est sous **LGPL 2.1**, et sa suite de tests sous **GPL 2**.
- Il y a donc **incohérence entre la déclaration et la réalité juridique**.

**Pourquoi ce n'est pas un problème aujourd'hui :** `default = []` → spandsp est **totalement inactif**. Il n'est compilé que si l'on active explicitement la feature `t38`, qui concerne le **fax T.38** — hors du périmètre d'opendial.

**Pourquoi c'est documenté :** si un contributeur souhaitait un jour ajouter le support du fax, il pourrait activer `t38` en toute bonne foi en se fiant à la métadonnée MIT, et **enfreindre la LGPL sans le savoir**. Cet ADR existe pour que ce piège soit connu avant d'être rencontré.

**Règle qui en découle :** ne jamais activer la feature `t38` de `rustrtc` sans audit préalable de l'amont spandsp et respect des obligations LGPL.

### Application — `cargo-deny` en CI

La politique est **automatisée**, pas laissée à la discipline humaine :

- **`cargo-deny`** — vérifie les licences, les vulnérabilités connues, les sources et les doublons de dépendances. Toute licence hors liste blanche **fait échouer la CI**.
- **`Cargo.lock` versionné** — c'est un binaire distribuable ; les versions résolues doivent être reproductibles et auditées.
- Toute nouvelle dépendance passe par une **revue explicite** dans la pull request, avec sa justification.

> L'installation de `cargo-deny` et la configuration de `deny.toml` sont à réaliser en Phase 0.

## Décision 3 — Inventaire des licences vérifiées

Licences des dépendances principales, **relevées à la version épinglée** le 2026-09-20 :

| Crate | Version | Licence | Statut |
|---|---|---|---|
| `rsipstack` | 0.6.10 | MIT | ✅ liste blanche |
| `rustrtc` | 0.3.138 | MIT | ✅ liste blanche — **feature `t38` exclue** |
| `cpal` | 0.18.2 | Apache-2.0 | ✅ liste blanche |
| `opus` | 0.4.0 | MIT / Apache-2.0 | ✅ liste blanche |
| `aec3` | 0.4.0 | MIT OR BSD-3-Clause | ✅ liste blanche |
| `flutter_rust_bridge` | 2.13.0 | MIT | ✅ liste blanche |
| `tokio` | 1.x | MIT | ✅ liste blanche |
| `serde` | 1.x | MIT OR Apache-2.0 | ✅ liste blanche |
| `thiserror` | 2.x | MIT OR Apache-2.0 | ✅ liste blanche |
| `tracing` | 0.1 | MIT | ✅ liste blanche |
| `parking_lot` | 0.12 | MIT OR Apache-2.0 | ✅ liste blanche |
| `dashmap` | 6.x | MIT | ✅ liste blanche |
| `socket2` | 0.6 | MIT OR Apache-2.0 | ✅ liste blanche |
| `rustls` | 0.23 | Apache-2.0 OR ISC OR MIT | ✅ liste blanche |
| `ring` | 0.17 | Apache-2.0 AND ISC | ✅ liste blanche |
| `spandsp-sys` | 0.1.5 | **MIT déclaré / LGPL-2.1 réel** | ⚠️ **écarté — feature non activée** |

**Note sur `aec3` :** ce crate est un portage Rust des algorithmes de WebRTC AEC3. En amont, `webrtc-audio-processing` (le projet de référence) est sous **BSD-3-Clause**, ce que la double licence `MIT OR BSD-3-Clause` reflète de façon cohérente. Cas sain, contrairement à spandsp.

**Note sur `cpal` :** la version actuelle est sous **Apache-2.0 seule** — les versions antérieures étaient en double licence MIT/Apache-2.0. Cela reste dans la liste blanche et ne change rien pour nous.

**Note sur l'écosystème WebRTC :** si `rustrtc` ou une dépendance future tirait du code de l'écosystème `webrtc-rs`, noter que celui-ci est sous **MIT/Apache-2.0**. Seul spandsp pose problème dans tout l'arbre de dépendances.

## Conséquences

### Positives

- **Liberté stratégique préservée.** Le double licence reste ouvert ; aucune décision irréversible n'a été prise.
- **Contamination empêchée par construction.** `cargo-deny` rend l'introduction accidentelle d'une licence copyleft **impossible sans échec de CI**, là où MicroSIP s'est soudé à PJSIP sans garde-fou.
- **Le piège spandsp est documenté** au lieu d'être une bombe à retardement.
- **Attractivité pour les intégrateurs.** Une entreprise peut embarquer opendial dans un produit fermé — ce qui est impossible avec Linphone (GPLv3) ou MicroSIP (GPLv2).

### Négatives — assumées

- **Tâche supplémentaire en CI.** `cargo-deny` ralentit légèrement les builds et ajoute une configuration à maintenir. Acceptable au regard du risque évité.
- **Frictions possibles sur certaines dépendances.** Une bibliothèque utile sous LGPL exigera un audit, voire une alternative. C'est une contrainte de développement réelle.
- **Le copyleft par fichier est imparfait.** Un tiers peut distribuer une version modifiée d'opendial en séparant ses modifications propriétaires dans des fichiers distincts. C'est la limite connue de MPL-2.0 — le prix de l'ouverture aux intégrateurs.

## Références

- [Mozilla Public License 2.0](https://mozilla.org/MPL/2.0/) — notamment §3.3 (Secondary License)
- [spandsp — FreeSWITCH](https://github.com/freeswitch/spandsp) — LGPL 2.1, suite de tests GPL 2
- [webrtc-audio-processing](https://github.com/tonarino/webrtc-audio-processing) — BSD-3-Clause
- [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) — outil d'audit automatisé
- ADR-0001 — réécriture propre plutôt que réutilisation de MicroSIP

> Ce document expose un raisonnement technique et juridique général ; il ne constitue pas un avis juridique. Pour une décision commerciale engageante, consulter un conseil spécialisé.

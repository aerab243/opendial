# opendial

Un softphone SIP open-source, cross-platform, écrit en **Flutter** (interface) et **Rust** (tout le temps réel).

> **Objectif** : offrir une alternative moderne à MicroSIP — plus rapide, plus
> légère, et surtout avec une architecture qui reste maintenable dans dix ans.

## État du projet

🚧 **Phase 0 — Fondations.** Le squelette est en place ; aucune fonctionnalité
de téléphonie n'est encore implémentée.

| Phase | Contenu | État |
|---|---|---|
| 0 | Fondations : workspace, CI, licence, banc de test | 🚧 en cours |
| 1 | Compte SIP et enregistrement (REGISTER) | ⬜ à venir |
| 2 | Premier appel, codec G.711 | ⬜ à venir |
| 3 | Opus, anti-écho, gestion des périphériques | ⬜ à venir |
| 4 | Interface complète : contacts, historique, réglages | ⬜ à venir |
| 5 | Transfert, mise en attente, multi-appels, NAT | ⬜ à venir |

## Architecture

```
┌─────────────────────────────────────────────────┐
│  Flutter (Dart)  —  interface uniquement        │
│  Composeur, écran d'appel, contacts, réglages   │
└────────────────────┬────────────────────────────┘
                     │ flutter_rust_bridge
                     │ événements ↑ / commandes ↓
┌────────────────────▼────────────────────────────┐
│  Rust  —  100 % du temps réel                   │
│  Signalisation SIP · RTP · codecs · AEC · audio │
└─────────────────────────────────────────────────┘
```

### La règle non négociable

> **Flutter ne se trouve jamais dans le chemin audio.**

Flutter gère une interface graphique, pas de l'audio temps réel : le ramasse-miettes de Dart et les canaux de communication introduisent des pauses incompatibles avec une conversation. L'interface *reflète* l'état ; elle ne le produit pas.

### Les crates

| Crate | Rôle | Dépend de |
|---|---|---|
| `od-core` | Domaine : types, machines à états, traits-ports. **Aucune I/O.** | rien |
| `od-sip` | Signalisation SIP (adaptateur `rsipstack`) | `od-core` |
| `od-media` | RTP, SRTP, jitter buffer (adaptateur `rustrtc`) | `od-core` |
| `od-audio` | Capture, lecture, anti-écho (`cpal`, `aec3`) | `od-core` |
| `od-config` | Comptes et préférences | `od-core` |
| `od-contacts` | Carnet d'adresses | `od-core` |
| `od-history` | Historique des appels | `od-core` |
| `od-ffi` | Façade vers Flutter. **Aucune logique.** | `od-core` |

**Les flèches ne s'inversent jamais.** Le domaine définit les traits ; les adaptateurs les implémentent. C'est ce qui permet de remplacer un stack SIP sans toucher au domaine ni à l'interface — voir [ADR-0002](docs/adr/) à venir.

## Pourquoi ne pas réutiliser MicroSIP

MicroSIP est sous **GPLv2**, soudé aux entrailles privées de PJSIP, et son modèle de données hérite de classes MFC — au point qu'un bug de logique d'appel exige d'ouvrir une fenêtre Windows pour être reproduit.

opendial est une **réécriture originale** : nous reprenons son ergonomie et sa couverture fonctionnelle, jamais son code. Le raisonnement complet est dans **[ADR-0001](docs/adr/0001-why-not-microsip.md)**.

## Compiler

### Prérequis

- **Rust** 1.98+ ([rustup](https://rustup.rs))
- **Flutter** 3.47+ stable
- **Linux** : paquets de développement

```bash
sudo apt install -y clang cmake ninja-build libgtk-3-dev libasound2-dev libopus-dev libclang-dev pkg-config
```

> Pour un audio sans craquements, ajoutez votre utilisateur au groupe `audio`
> (priorité temps réel), puis reconnectez-vous :
>
> ```bash
> sudo usermod -aG audio $USER
> ```

### Construire et tester

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## Banc de test

Un serveur Asterisk local, avec deux comptes préconfigurés :

```bash
docker compose -f testbed/docker-compose.yml up -d
```

Vérifier que les postes sont connus :

```bash
docker compose -f testbed/docker-compose.yml exec asterisk asterisk -rx "pjsip show endpoints"
```

| Compte | Utilisateur | Mot de passe |
|---|---|---|
| 1001 | `1001` | `opendial` |
| 1002 | `1002` | `opendial` |

Numéros de test : **600** écho, **601** annonce.

> ⚠️ Ce serveur utilise des mots de passe triviaux et n'est **pas** sécurisé.
> Il est destiné au développement local, jamais à être exposé sur un réseau.

## Licence

**MPL-2.0** — voir [LICENSE](LICENSE) et [ADR-0003](docs/adr/0003-licence-et-politique-dependances.md).

En résumé : les modifications des fichiers d'opendial restent ouvertes, mais
l'intégration dans un produit à code fermé est permise. Le projet reste
compatible GPL/LGPL/AGPL pour s'intégrer à l'écosystème télécom existant.

## Contribuer

Avant toute contribution, lire **[ADR-0001](docs/adr/0001-why-not-microsip.md)** :
nous ne copions **aucun** code de MicroSIP, et consulter son code source pendant
l'écriture du nôtre crée un risque juridique.

Les décisions structurantes sont consignées dans [`docs/adr/`](docs/adr/).

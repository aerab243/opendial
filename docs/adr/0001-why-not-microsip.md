# ADR-0001 — Réécriture propre plutôt que réutilisation de MicroSIP

- **Date** : 2026-09-20
- **Statut** : Accepté
- **Décideurs** : mainteneurs d'opendial

## Contexte

opendial est un softphone SIP open-source cross-platform (Flutter + Rust). Son objectif explicite est d'offrir une alternative à **MicroSIP** : plus rapide, plus performante, mieux structurée et plus agréable à utiliser.

MicroSIP est le softphone de référence sur Windows depuis 2011. Il est donc naturel de se demander si l'on peut partir de son code source. Cette question s'est posée dès l'initialisation du projet et mérite une réponse documentée, car :

1. elle détermine le **choix de licence** d'opendial, de façon irréversible ;
2. elle détermine l'**architecture** du projet ;
3. un contributeur se la reposera inévitablement, et pourrait commettre une erreur aux conséquences juridiques durables.

Ce document répond une fois pour toutes, sur la base du code source réel de MicroSIP.

## Examen

### Constat 1 — MicroSIP est sous GNU GPL v2

Chaque fichier source porte l'en-tête suivant :

```cpp
/* Copyright (C) 2011-2020 MicroSIP (http://www.microsip.org)
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 */
```

MicroSIP est bâti sur **PJSIP**, lui-même distribué sous **GPLv2 ou licence commerciale Teluu**. Ces deux copylefts se propagent à toute œuvre dérivée.

**Conséquence directe :** reprendre du code MicroSIP forcerait opendial à être GPLv2. Cela interdirait définitivement :
- toute intégration dans un produit à code fermé ;
- toute formule de double licence (le modèle de Linphone) ;
- toute compatibilité avec un SDK commercial.

Le choix de licence étant structurant et difficilement réversible, ce point seul suffit à écarter la réutilisation.

### Constat 2 — L'architecture de MicroSIP n'est pas transposable

L'examen des 110 fichiers sources révèle trois couplages structurels :

**a) Le modèle de données est une fenêtre Windows.**

```cpp
class Calls : public CBaseDialog        // Calls.h — la gestion des appels EST une boîte de dialogue
class CmainDlg : public CBaseDialog     // mainDlg.h — l'état applicatif vit dans la fenêtre principale
```

**b) Toute la logique transite par des messages Windows.**

```cpp
afx_msg LRESULT onCallState(WPARAM wParam, LPARAM lParam);
afx_msg LRESULT onIncomingCall(WPARAM wParam, LPARAM lParam);
afx_msg LRESULT onRegState2(WPARAM wParam, LPARAM lParam);
afx_msg LRESULT onCallMediaState(WPARAM, LPARAM);
afx_msg LRESULT onCallTransferStatus(WPARAM, LPARAM);
```

**c) MicroSIP dépend des entrailles privées de PJSIP.**

```cpp
#include <pjsua-lib/pjsua_internal.h>              // lib/MSIP.h — en-tête INTERNE
if (pjsua_var.state == PJSUA_STATE_RUNNING)        // microsip.cpp:132 — variable globale privée
```

`pjsua_internal.h` et `pjsua_var` ne font pas partie de l'API publique de PJSIP. MicroSIP dépend donc de détails d'implémentation que Teluu peut modifier sans préavis.

**Conséquences techniques :**

| Problème | Impact |
|---|---|
| Domaine couplé à `CString` (MFC) | La logique métier est **intestable sans ouvrir une fenêtre Windows** |
| Événements = messages Windows | Aucun équivalent en Rust asynchrone ; le portage serait une réécriture complète |
| Structure à plat, 110 fichiers | UI et logique mêlées ; aucune frontière stable |
| `pjsua_internal.h` | **Changement de stack SIP impossible sans tout réécrire** |

L'architecture de MicroSIP est un cul-de-sac qui date de 2011. Elle n'est ni portable, ni testable, ni remplaçable.

### Constat 3 — Un point de droit : idées et expression

Le droit d'auteur distingue deux objets :

- **Les idées, fonctionnalités et comportements** ne sont **pas** protégeables. Une maison à trois chambres avec cuisine à l'est peut être reproduite.
- **L'expression** l'est : le code source, et selon la jurisprudence aussi la **structure, la séquence et l'organisation** non littérales d'un programme (test AFC — *Abstraction-Filtration-Comparison*, établi par *Computer Associates v. Altai*).

Ce second point est le plus important et le plus souvent ignoré : **on ne peut pas se contenter de renommer les symboles** en conservant l'organisation de MicroSIP. Réécrire `Calls` en `CallManager` en gardant la même décomposition serait une contrefaçon.

À l'inverse, deux éléments nous protègent solidement :

- **La doctrine de la fusion** : lorsqu'une expression est dictée par une spécification technique, elle n'est pas protégeable — il n'existe qu'une façon correcte d'implémenter `INVITE` ou le digest auth.
- **Les scènes nécessaires** (*scènes à faire*) : les comportements imposés par un standard sont exclus de la protection.

Le comportement d'un softphone SIP est **normalisé par les RFC** (3261, 3262, 3264, 3550, 3711, 2833). La structure, elle, ne l'est pas — et c'est précisément là que se situe notre travail d'architecture.

## Décision

**opendial est une réécriture originale. Aucun code source, aucune ressource graphique et aucune organisation de fichiers de MicroSIP ne sera reprise.**

Nous reprenons de MicroSIP :
- son **comportement fonctionnel** et son **ergonomie** — ce qui marche, ce qui manque, ce que les utilisateurs attendent ;
- sa **couverture fonctionnelle** comme cahier des charges ;
- les **leçons de ses défauts** : c'est en constatant ses couplages que nous définissons les frontières du projet.

Nous ne reprenons pas :
- une seule ligne de code ;
- une seule icône, image ou ressource ;
- la décomposition en classes ni l'organisation des fichiers ;
- PJSIP, ni aucune de ses dépendances.

### Règles pratiques pour les contributeurs

| Autorisé ✅ | Interdit ❌ |
|---|---|
| Lire MicroSIP pour comprendre un comportement | Copier/coller du code, même adapté |
| Consulter sa documentation et ses rapports de bugs | Reproduire sa décomposition en classes |
| S'inspirer de son ergonomie et de ses workflows | Reprendre ses ressources graphiques |
| Implémenter une RFC comme il la respecte | Lire son code pendant qu'on écrit le nôtre |
| Vérifier une interopérabilité par test | Cloner son dépôt dans l'arborescence du projet |

> **Règle de sécurité** : un contributeur qui consulte le code de MicroSIP pendant qu'il écrit du code opendial crée un risque juridique. Consulter sa **documentation** ou **tester son comportement** est sûr ; lire son **code** en cours d'implémentation ne l'est pas.

## Conséquences

### Positives

- **Liberté de licence totale.** opendial adopte **MPL-2.0** : copyleft par fichier, compatible avec l'intégration propriétaire, et ouvrant la porte au double licence.
- **Dépendances sous licence permissive.** Nous utilisons `rsipstack` et `rustrtc` (**MIT**), `cpal` et `opus` (**MIT/Apache-2.0**). Aucune contamination copyleft, et **PJSIP n'est pas nécessaire** — c'est lui qui imposait GPLv2 à MicroSIP.
- **Domaine testable.** Le domaine vit dans `od-core`, en Rust pur, sans I/O. Un bug de logique d'appel se reproduit par `cargo test` en deux secondes, là où MicroSIP exige d'ouvrir une fenêtre Windows et de cliquer.
- **Stack SIP remplaçable.** `od-sip` est un adaptateur derrière un trait du domaine. Là où MicroSIP est définitivement soudé à PJSIP par `pjsua_internal.h`, nous pouvons changer d'implémentation sans toucher au domaine ni à l'UI.

### Négatives — assumées

- **Coût de réécriture intégral.** Chaque fonctionnalité doit être réimplémentée et testée. C'est le prix de la liberté de licence.
- **Dette d'interopérabilité.** MicroSIP a quinze ans de correctifs issus du terrain (NAT, IPBX exotiques, cas limites SDP). Nous redécouvrirons certains de ces problèmes. Mitigation : la validation contre le **corpus de torture RFC 4475** et les tests d'interopérabilité avec de vrais clients.
- **Obligation de vigilance.** Le risque de contamination accidentelle est réel. Cette règle doit être rappelée à chaque contribution.

### Architecture imposée par cette décision

Puisque `rsipstack` et `rustrtc` sont des crates **0.x maintenus par un seul auteur**, le même raisonnement s'applique à eux : ne jamais en devenir captif. D'où le découpage ports & adapters :

```
od-core      domaine pur + traits-ports      (aucune dépendance externe)
   ↑
od-sip       adaptateur rsipstack            (remplaçable)
od-media     adaptateur rustrtc              (remplaçable)
od-audio     cpal + aec3
od-ffi       façade mince vers Flutter       (aucune logique)
```

C'est la leçon centrale de MicroSIP : **un projet qui se soude aux entrailles d'une dépendance tierce ne peut plus évoluer.** Voir ADR-0002.

## Références

- Code source de MicroSIP : `Calls.h`, `mainDlg.h`, `lib/MSIP.h`, `microsip.cpp`
- [PJSIP Licensing](https://www.pjsip.org/licensing.htm) — GPLv2 ou licence commerciale Teluu
- [MicroSIP Source Code](https://www.microsip.org/source) — GPLv2
- RFC 3261 (SIP), 3262 (100rel/PRACK), 3264 (offer/answer), 3550 (RTP), 3711 (SRTP), 2833 (DTMF)
- *Computer Associates v. Altai* — test AFC sur la protection de la structure non littérale

> Ce document expose un raisonnement technique et juridique général ; il ne constitue pas un avis juridique. Pour une décision commerciale engageante, consulter un conseil spécialisé.

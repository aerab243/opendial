// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Compte SIP : identité, credentials et serveur d'enregistrement.
//!
//! Un `Account` décrit **comment joindre un serveur SIP** et **sous quelle
//! identité**. Il ne contient ni socket, ni état d'enregistrement : cet état
//! vit dans [`RegistrationState`](super::registration::RegistrationState), et
//! les identifiants de connexion dans
//! [`Credentials`], dont le `Debug` masque le mot de passe.

use std::fmt;
use std::num::NonZeroU32;

use crate::error::DomainError;

/// Identifiant stable d'un compte, attribué par l'application.
///
/// C'est une chaîne opaque : le domaine ne présume ni d'un UUID, ni d'un index
/// de base de données. La seule règle est qu'un identifiant vide n'est pas
/// valide.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(String);

impl AccountId {
    /// Crée un identifiant de compte.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`DomainError::EmptyAccountId`] si la chaîne est vide ou
    /// composée uniquement d'espaces.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::EmptyAccountId);
        }
        Ok(Self(value))
    }

    /// Renvoie l'identifiant sous forme de chaîne.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Transport utilisé pour joindre le serveur SIP.
///
/// `Udp` est le défaut historique du monde SIP ; `Tls` est le seul à chiffrer
/// la signalisation, et le seul acceptable hors réseau de confiance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    /// UDP — défaut, non chiffré.
    #[default]
    Udp,
    /// TCP — non chiffré, mais fiable et traverse mieux certains pare-feu.
    Tcp,
    /// TLS — chiffré. À préférer dès que le serveur le supporte.
    Tls,
}

impl Transport {
    /// Renvoie le nom du transport tel qu'il apparaît dans une URI SIP.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Tls => "tls",
        }
    }

    /// Indique si le transport chiffre la signalisation.
    #[must_use]
    pub const fn is_secure(self) -> bool {
        matches!(self, Self::Tls)
    }
}

/// Serveur d'enregistrement (registrar) d'un compte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registrar {
    /// Hôte ou adresse IP du serveur.
    pub host: String,
    /// Port du serveur, si non standard pour le transport choisi.
    pub port: Option<u16>,
    /// Transport à utiliser.
    pub transport: Transport,
}

impl Registrar {
    /// Crée un registrar sur l'hôte donné, avec le transport par défaut (UDP).
    ///
    /// # Erreurs
    ///
    /// Renvoie [`DomainError::EmptyRegistrarHost`] si l'hôte est vide, ou
    /// [`DomainError::InvalidRegistrarHost`] s'il contient des caractères
    /// interdits dans un nom d'hôte.
    pub fn new(host: impl Into<String>) -> Result<Self, DomainError> {
        let host = host.into();
        let trimmed = host.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyRegistrarHost);
        }
        // Un nom d'hôte ne peut contenir ni espace, ni barre oblique, ni
        // crochet (une adresse IPv6 littérale se note entre crochets dans une
        // URI, jamais dans le champ hôte — c'est l'adaptateur qui l'ajoute).
        if trimmed.contains([' ', '/', '\\']) {
            return Err(DomainError::InvalidRegistrarHost(host));
        }
        Ok(Self {
            host: trimmed.to_owned(),
            port: None,
            transport: Transport::default(),
        })
    }

    /// Fixe le port du serveur.
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Fixe le transport à utiliser.
    #[must_use]
    pub fn with_transport(mut self, transport: Transport) -> Self {
        self.transport = transport;
        self
    }
}

impl fmt::Display for Registrar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.transport.as_str(), self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        Ok(())
    }
}

/// Identifiants d'authentification SIP (digest, RFC 3261 §22.4).
///
/// Le mot de passe n'apparaît jamais dans `Debug` ni dans `Display` : une trace
/// de journal ou un rapport de bug ne peut donc pas le divulguer. L'accès se
/// fait uniquement par [`Credentials::password`], explicitement.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// Nom d'utilisateur pour l'authentification.
    ///
    /// Peut différer de l'identifiant d'enregistrement : certains serveurs
    /// exigent un `auth_id` distinct de l'URI d'enregistrement.
    pub username: String,
    password: String,
    /// Domaine d'authentification, si le serveur l'impose.
    ///
    /// Laisser à `None` pour découvrir le realm depuis le défi `401`/`407`.
    pub realm: Option<String>,
}

impl Credentials {
    /// Crée des identifiants d'authentification.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`DomainError::EmptyUsername`] si le nom d'utilisateur est vide.
    /// Un mot de passe vide est accepté : certains serveurs de test
    /// fonctionnent ainsi, et refuser ici empêcherait de les joindre.
    pub fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let username = username.into();
        if username.trim().is_empty() {
            return Err(DomainError::EmptyUsername);
        }
        Ok(Self {
            username,
            password: password.into(),
            realm: None,
        })
    }

    /// Fixe le domaine d'authentification.
    #[must_use]
    pub fn with_realm(mut self, realm: impl Into<String>) -> Self {
        self.realm = Some(realm.into());
        self
    }

    /// Renvoie le mot de passe.
    ///
    /// Méthode volontairement explicite : le mot de passe ne se propage pas par
    /// accident dans un `Debug`, une sérialisation ou un log.
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"<masqué>")
            .field("realm", &self.realm)
            .finish()
    }
}

/// Durée d'enregistrement avant rafraîchissement.
///
/// Valeur non nulle par construction : une expiration nulle signifie « se
/// désenregistrer » (RFC 3261 §10.2.2) et relève d'une action explicite, pas
/// d'une configuration de compte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Expiry(NonZeroU32);

impl Expiry {
    /// Durée d'enregistrement par défaut, en secondes.
    ///
    /// 300 s est une valeur courante et prudente : assez courte pour qu'une
    /// reprise après coupure réseau soit rapide, assez longue pour ne pas
    /// marteler le serveur.
    pub const DEFAULT_SECS: u32 = 300;

    /// Crée une durée d'expiration en secondes.
    ///
    /// # Erreurs
    ///
    /// Renvoie [`DomainError::ZeroExpiry`] si `seconds` vaut zéro.
    pub fn from_secs(seconds: u32) -> Result<Self, DomainError> {
        NonZeroU32::new(seconds)
            .map(Self)
            .ok_or(DomainError::ZeroExpiry)
    }

    /// Renvoie la durée en secondes.
    #[must_use]
    pub const fn as_secs(self) -> u32 {
        self.0.get()
    }
}

impl Default for Expiry {
    fn default() -> Self {
        // La valeur par défaut est non nulle : l'invariant du type est préservé
        // sans `expect`, qui serait un point de panique injustifiable.
        Self(NonZeroU32::new(Self::DEFAULT_SECS).unwrap_or(NonZeroU32::MIN))
    }
}

/// Compte SIP complet, prêt à être enregistré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// Identifiant stable, attribué par l'application.
    pub id: AccountId,
    /// Libellé affiché à l'utilisateur (ex. « Travail »).
    pub label: String,
    /// URI d'enregistrement, de la forme `utilisateur@domaine`.
    pub address_of_record: String,
    /// Identifiants d'authentification.
    pub credentials: Credentials,
    /// Serveur d'enregistrement.
    pub registrar: Registrar,
    /// Durée d'enregistrement souhaitée.
    pub expiry: Expiry,
}

impl Account {
    /// Crée un compte en validant l'ensemble de ses champs.
    ///
    /// # Erreurs
    ///
    /// Renvoie la première [`DomainError`] rencontrée : libellé vide, adresse
    /// d'enregistrement sans domaine.
    pub fn new(
        id: AccountId,
        label: impl Into<String>,
        address_of_record: impl Into<String>,
        credentials: Credentials,
        registrar: Registrar,
    ) -> Result<Self, DomainError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(DomainError::EmptyAccountLabel);
        }

        let address_of_record = address_of_record.into();
        // Une URI d'enregistrement doit porter un domaine : « alice » seul ne
        // permet à aucun adaptateur de construire une requête REGISTER valide.
        if !address_of_record.contains('@') {
            return Err(DomainError::InvalidAddressOfRecord(address_of_record));
        }

        Ok(Self {
            id,
            label,
            address_of_record,
            credentials,
            registrar,
            expiry: Expiry::default(),
        })
    }

    /// Fixe la durée d'enregistrement.
    #[must_use]
    pub fn with_expiry(mut self, expiry: Expiry) -> Self {
        self.expiry = expiry;
        self
    }

    /// Renvoie la partie utilisateur de l'URI d'enregistrement.
    ///
    /// Renvoie une chaîne vide si l'adresse ne contient pas de `@`, cas que
    /// [`Account::new`] interdit — la méthode reste donc totalement sûre.
    #[must_use]
    pub fn username(&self) -> &str {
        self.address_of_record
            .split_once('@')
            .map_or("", |(user, _)| user)
    }

    /// Renvoie la partie domaine de l'URI d'enregistrement.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.address_of_record
            .split_once('@')
            .map_or("", |(_, domain)| domain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> Credentials {
        Credentials::new("1001", "secret").expect("identifiants valides")
    }

    fn registrar() -> Registrar {
        Registrar::new("pbx.example.com").expect("hôte valide")
    }

    #[test]
    fn account_id_rejects_blank_values() {
        assert_eq!(AccountId::new(""), Err(DomainError::EmptyAccountId));
        assert_eq!(AccountId::new("   "), Err(DomainError::EmptyAccountId));
        assert_eq!(AccountId::new("  \t\n "), Err(DomainError::EmptyAccountId));
    }

    #[test]
    fn account_id_trims_nothing_but_preserves_value() {
        // Un identifiant peut contenir des espaces internes : seul le cas
        // « entièrement vide » est invalide.
        let id = AccountId::new("compte 1").expect("identifiant valide");
        assert_eq!(id.as_str(), "compte 1");
    }

    #[test]
    fn registrar_rejects_invalid_hosts() {
        assert_eq!(Registrar::new(""), Err(DomainError::EmptyRegistrarHost));
        assert_eq!(Registrar::new("  "), Err(DomainError::EmptyRegistrarHost));
        assert!(matches!(
            Registrar::new("host invalide"),
            Err(DomainError::InvalidRegistrarHost(_))
        ));
        assert!(matches!(
            Registrar::new("sip://pbx"),
            Err(DomainError::InvalidRegistrarHost(_))
        ));
    }

    #[test]
    fn registrar_display_includes_scheme_and_optional_port() {
        let simple = Registrar::new("pbx.example.com").expect("hôte valide");
        assert_eq!(simple.to_string(), "udp://pbx.example.com");

        let full = Registrar::new("pbx.example.com")
            .expect("hôte valide")
            .with_port(5061)
            .with_transport(Transport::Tls);
        assert_eq!(full.to_string(), "tls://pbx.example.com:5061");
        assert!(full.transport.is_secure());
    }

    #[test]
    fn credentials_never_leak_password_in_debug() {
        let creds = credentials();
        let rendered = format!("{creds:?}");
        assert!(
            !rendered.contains("secret"),
            "le mot de passe apparaît dans Debug : {rendered}"
        );
        assert!(rendered.contains("<masqué>"));
        // L'accès explicite reste possible.
        assert_eq!(creds.password(), "secret");
    }

    #[test]
    fn credentials_reject_blank_username() {
        assert_eq!(
            Credentials::new("", "peu importe"),
            Err(DomainError::EmptyUsername)
        );
        // Un mot de passe vide est accepté (serveurs de test).
        assert!(Credentials::new("1001", "").is_ok());
    }

    #[test]
    fn expiry_cannot_be_zero() {
        assert_eq!(Expiry::from_secs(0), Err(DomainError::ZeroExpiry));
        assert_eq!(Expiry::from_secs(120).map(Expiry::as_secs), Ok(120));
        assert_eq!(Expiry::default().as_secs(), Expiry::DEFAULT_SECS);
    }

    #[test]
    fn account_requires_domain_in_address_of_record() {
        let id = AccountId::new("a1").expect("identifiant valide");
        let result = Account::new(id.clone(), "Travail", "1001", credentials(), registrar());
        assert!(matches!(
            result,
            Err(DomainError::InvalidAddressOfRecord(_))
        ));

        let account = Account::new(
            id,
            "Travail",
            "1001@pbx.example.com",
            credentials(),
            registrar(),
        )
        .expect("compte valide");
        assert_eq!(account.username(), "1001");
        assert_eq!(account.domain(), "pbx.example.com");
    }

    #[test]
    fn account_rejects_blank_label() {
        let id = AccountId::new("a1").expect("identifiant valide");
        assert_eq!(
            Account::new(id, "  ", "1001@pbx.example.com", credentials(), registrar()),
            Err(DomainError::EmptyAccountLabel)
        );
    }
}

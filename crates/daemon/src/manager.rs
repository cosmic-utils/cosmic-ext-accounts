use crate::{
    Error,
    auth::{AuthManager, PasswordKind},
    request::{RequestInterface, RequestState, RequestStatus, request_object_path},
};
use accounts_core::{
    config::AccountsConfig,
    models::{
        Account, CUSTOM_CALDAV_PROVIDER, CUSTOM_CARDDAV_PROVIDER, CUSTOM_LDAP_PROVIDER,
        CUSTOM_MAIL_PROVIDER, ContactsConnectionSettings, CustomMailSettings, Service,
    },
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;
use zbus::{
    Connection,
    fdo::Result,
    interface,
    message::Header,
    object_server::SignalEmitter,
    zvariant::{ObjectPath, OwnedObjectPath},
};

/// Keep terminal requests registered so callers can read their final state after the signal.
const REQUEST_GRACE_PERIOD: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(crate) fn validate_password(password: &str) -> Result<()> {
    if password.is_empty() || password.contains(['\r', '\n', '\0']) {
        return Err(zbus::fdo::Error::InvalidArgs(
            "Enter a valid password".into(),
        ));
    }
    Ok(())
}

fn custom_mail_account(
    email: &str,
    username: &str,
    mut settings: CustomMailSettings,
) -> Result<Account> {
    use zbus::fdo::Error;

    let email = email.trim();
    let valid_email = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && !domain.is_empty() && !domain.contains('@')
    }) && !email
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || "<>,;".contains(c));
    if !valid_email {
        return Err(Error::InvalidArgs("Enter a valid email address".into()));
    }
    let username = username.trim();
    if username.chars().any(char::is_control) {
        return Err(Error::InvalidArgs("Enter a valid mail username".into()));
    }
    if settings.imap_port == 0 || settings.smtp_port == 0 {
        return Err(Error::InvalidArgs(
            "Enter IMAP and SMTP ports between 1 and 65535".into(),
        ));
    }
    settings.imap_host = mail_host(&settings.imap_host)?;
    settings.smtp_host = mail_host(&settings.smtp_host)?;

    Ok(Account {
        id: Uuid::new_v4(),
        provider: CUSTOM_MAIL_PROVIDER.into(),
        display_name: email.into(),
        username: if username.is_empty() { email } else { username }.into(),
        email: Some(email.into()),
        enabled: true,
        created_at: chrono::Utc::now(),
        last_used: None,
        services: [(Service::Email, true)].into(),
        custom_mail: Some(settings),
        caldav_url: None,
        custom_contacts: None,
    })
}

fn custom_caldav_account(url: &str, username: &str) -> Result<Account> {
    use zbus::fdo::Error;

    let url = password_server_url(url, "https").ok_or_else(|| {
        Error::InvalidArgs(
            "Enter an HTTPS CalDAV URL without credentials, a query, or a fragment".into(),
        )
    })?;

    let username = username.trim();
    if username.is_empty() || username.contains(':') || username.chars().any(char::is_control) {
        return Err(Error::InvalidArgs("Enter a valid CalDAV username".into()));
    }

    Ok(Account {
        id: Uuid::new_v4(),
        provider: CUSTOM_CALDAV_PROVIDER.into(),
        display_name: username.into(),
        username: username.into(),
        email: None,
        enabled: true,
        created_at: chrono::Utc::now(),
        last_used: None,
        services: [(Service::Calendar, true)].into(),
        custom_mail: None,
        caldav_url: Some(url.into()),
        custom_contacts: None,
    })
}

fn password_server_url(value: &str, scheme: &str) -> Option<url::Url> {
    url::Url::parse(value.trim()).ok().filter(|url| {
        url.scheme() == scheme
            && url.host().is_some()
            && url.port() != Some(0)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

fn custom_contacts_account(
    mut settings: ContactsConnectionSettings,
    username: &str,
) -> Result<Account> {
    use zbus::fdo::Error;

    let (provider, scheme) = match settings.protocol.as_str() {
        "carddav" => (CUSTOM_CARDDAV_PROVIDER, "https"),
        "ldap" => (CUSTOM_LDAP_PROVIDER, "ldaps"),
        _ => return Err(Error::InvalidArgs("Unsupported contacts protocol".into())),
    };
    let mut uri = password_server_url(&settings.uri, scheme).ok_or_else(|| {
        Error::InvalidArgs(format!(
            "Enter a {scheme}:// URL without credentials, a query, or a fragment"
        ))
    })?;
    let username = username.trim();
    if username.is_empty()
        || username.chars().any(char::is_control)
        || (provider == CUSTOM_CARDDAV_PROVIDER && username.contains(':'))
    {
        return Err(Error::InvalidArgs(
            "Enter a valid username or LDAP bind DN".into(),
        ));
    }

    settings.base_dn = settings.base_dn.trim().to_owned();
    if provider == CUSTOM_LDAP_PROVIDER {
        if !matches!(uri.path(), "" | "/") {
            return Err(Error::InvalidArgs(
                "Enter the LDAP base DN separately from the server URL".into(),
            ));
        }
        if settings.base_dn.is_empty() || settings.base_dn.chars().any(char::is_control) {
            return Err(Error::InvalidArgs("Enter an LDAP base DN".into()));
        }
        uri.set_path("");
    } else if !settings.base_dn.is_empty() {
        return Err(Error::InvalidArgs("A base DN is only used for LDAP".into()));
    }
    settings.uri = uri.into();

    Ok(Account {
        id: Uuid::new_v4(),
        provider: provider.into(),
        display_name: username.into(),
        username: username.into(),
        email: None,
        enabled: true,
        created_at: chrono::Utc::now(),
        last_used: None,
        services: [(Service::Contacts, true)].into(),
        custom_mail: None,
        caldav_url: None,
        custom_contacts: Some(settings),
    })
}

fn mail_host(value: &str) -> Result<String> {
    let value = value.trim();
    if let Ok(address) = value.parse::<std::net::IpAddr>() {
        return Ok(address.to_string());
    }
    if !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "/\\:@?#%".contains(c))
        && let Ok(host) = url::Host::parse(value)
    {
        return Ok(host.to_string());
    }
    Err(zbus::fdo::Error::InvalidArgs(
        "Enter a server hostname or IP address".into(),
    ))
}

pub fn account_object_path(id: &uuid::Uuid) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!(
        "/dev/edfloreshz/Accounts/Accounts/{}",
        id.to_string().replace('-', "_")
    ))
    .expect("account object path is always a valid path")
}

pub struct ManagerInterface {
    pub(crate) config: Arc<Mutex<AccountsConfig>>,
    pub(crate) auth_manager: Arc<Mutex<AuthManager>>,
}

impl ManagerInterface {
    pub fn new(config: Arc<Mutex<AccountsConfig>>, auth_manager: Arc<Mutex<AuthManager>>) -> Self {
        Self {
            config,
            auth_manager,
        }
    }

    async fn create_password_account(
        &self,
        header: Header<'_>,
        connection: &Connection,
        account: Account,
        kind: PasswordKind,
        password: &str,
    ) -> Result<OwnedObjectPath> {
        use zbus::fdo::Error;

        if !crate::polkit::check(&header, crate::polkit::ACTION_MANAGE_OWN_ACCOUNTS).await {
            return Err(Error::AccessDenied("Not authorized to add accounts".into()));
        }
        let grants = crate::GRANTS
            .get()
            .ok_or_else(|| Error::Failed("Accounts is starting".into()))?;
        let stale = crate::STALE
            .get()
            .ok_or_else(|| Error::Failed("Accounts is starting".into()))?;
        validate_password(password)?;

        let mut auth = self.auth_manager.lock().await;
        {
            let mut config = self.config.lock().await;
            if config.accounts.iter().any(|saved| {
                saved.provider == account.provider
                    && saved.username == account.username
                    && saved.email == account.email
                    && saved.custom_mail == account.custom_mail
                    && saved.caldav_url == account.caldav_url
                    && saved.custom_contacts == account.custom_contacts
            }) {
                return Err(Error::InvalidArgs("That account already exists".into()));
            }

            auth.set_account_password(&account.id, kind, password)
                .await
                .map_err(|_| Error::Failed("Could not save the password to the keyring".into()))?;
            if let Err(err) = config.save_account(&account) {
                let _ = auth.delete_credentials(&account.id).await;
                return Err(Error::Failed(err.to_string()));
            }
        }
        let result = crate::register_new_account(
            connection,
            &self.config,
            &self.auth_manager,
            grants,
            stale,
            &account,
        )
        .await;
        if result.is_err() {
            let _ = self.config.lock().await.remove_account(&account.id);
            let _ = auth.delete_credentials(&account.id).await;
        }
        result
    }
}

#[interface(name = "dev.edfloreshz.Accounts.Manager")]
impl ManagerInterface {
    async fn list_accounts(&self) -> Result<Vec<OwnedObjectPath>> {
        let config = self.config.lock().await;
        Ok(config
            .accounts
            .iter()
            .map(|account| account_object_path(&account.id))
            .collect())
    }

    async fn list_providers(&self) -> Result<Vec<OwnedObjectPath>> {
        let Some(registry) = crate::REGISTRY.get() else {
            return Ok(Vec::new());
        };
        Ok(registry
            .list()
            .into_iter()
            .filter_map(|manifest| {
                ObjectPath::try_from(format!(
                    "/dev/edfloreshz/Accounts/Providers/{}",
                    manifest.provider.id
                ))
                .ok()
                .map(|p| p.into())
            })
            .collect())
    }

    #[zbus(property)]
    async fn version(&self) -> Result<String> {
        Ok(env!("CARGO_PKG_VERSION").to_string())
    }

    #[zbus(property)]
    fn mail_endpoint_version(&self) -> u32 {
        1
    }

    async fn create_mail_account(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        email: &str,
        username: &str,
        settings: CustomMailSettings,
        password: &str,
    ) -> Result<OwnedObjectPath> {
        let account = custom_mail_account(email, username, settings)?;
        self.create_password_account(header, connection, account, PasswordKind::Mail, password)
            .await
    }

    async fn create_caldav_account(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<OwnedObjectPath> {
        let account = custom_caldav_account(url, username)?;
        self.create_password_account(header, connection, account, PasswordKind::CalDav, password)
            .await
    }

    async fn create_contacts_account(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        settings: ContactsConnectionSettings,
        username: &str,
        password: &str,
    ) -> Result<OwnedObjectPath> {
        let account = custom_contacts_account(settings, username)?;
        self.create_password_account(
            header,
            connection,
            account,
            PasswordKind::Contacts,
            password,
        )
        .await
    }

    async fn create_account(
        &self,
        provider_id: &str,
        params: HashMap<String, String>,
    ) -> Result<OwnedObjectPath> {
        if matches!(
            provider_id,
            CUSTOM_MAIL_PROVIDER
                | CUSTOM_CALDAV_PROVIDER
                | CUSTOM_CARDDAV_PROVIDER
                | CUSTOM_LDAP_PROVIDER
        ) {
            return Err(zbus::fdo::Error::NotSupported(
                "Use the dedicated custom account method".into(),
            ));
        }
        let Some(registry) = crate::REGISTRY.get() else {
            return Err(Error::InvalidProviderConfig.into());
        };
        if registry.get(provider_id).is_none() {
            return Err(Error::InvalidProvider(provider_id.to_string()).into());
        }

        create_request(
            provider_id.to_string(),
            None,
            params,
            self.auth_manager.clone(),
        )
        .await
    }

    #[zbus(signal)]
    pub(crate) async fn account_added(
        emitter: &SignalEmitter<'_>,
        account: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(crate) async fn account_removed(
        emitter: &SignalEmitter<'_>,
        account: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(crate) async fn account_toggled(
        emitter: &SignalEmitter<'_>,
        account: OwnedObjectPath,
        enabled: bool,
    ) -> zbus::Result<()>;
}

/// Starts sign-in for a new or existing account and returns its request path.
pub(crate) async fn create_request(
    provider_id: String,
    existing_account: Option<Uuid>,
    params: HashMap<String, String>,
    auth_manager: Arc<Mutex<AuthManager>>,
) -> Result<OwnedObjectPath> {
    let Some(connection) = crate::CONNECTION.get() else {
        return Err(Error::InvalidProviderConfig.into());
    };
    let Some(requests) = crate::REQUESTS.get() else {
        return Err(Error::InvalidProviderConfig.into());
    };

    let request_id = Uuid::new_v4().to_string();
    let path = request_object_path(&request_id);
    let state = Arc::new(Mutex::new(RequestState::default()));

    requests
        .lock()
        .await
        .insert(request_id.clone(), state.clone());

    connection
        .object_server()
        .at(
            path.clone(),
            RequestInterface::new(request_id.clone(), state.clone(), auth_manager.clone()),
        )
        .await?;

    let uses_handler = crate::REGISTRY
        .get()
        .and_then(|registry| registry.get(&provider_id))
        .is_some_and(|manifest| manifest.handler.is_some());

    if uses_handler {
        tokio::spawn(run_handler_flow(
            provider_id,
            existing_account,
            params,
            request_id,
            state,
            auth_manager,
            connection.clone(),
        ));
    } else {
        tokio::spawn(run_oauth_flow(
            provider_id,
            existing_account,
            params,
            request_id,
            state,
            auth_manager,
            connection.clone(),
        ));
    }

    Ok(path)
}

async fn run_handler_flow(
    provider_id: String,
    existing_account: Option<Uuid>,
    params: HashMap<String, String>,
    request_id: String,
    state: Arc<Mutex<RequestState>>,
    auth_manager: Arc<Mutex<AuthManager>>,
    connection: Connection,
) {
    tokio::spawn(terminal_timeout(
        request_id.clone(),
        state.clone(),
        connection.clone(),
    ));

    let completed = auth_manager
        .lock()
        .await
        .authenticate_via_handler(provider_id, request_id.clone(), existing_account, params)
        .await;

    let completed = match completed {
        Ok(completed) => completed,
        Err(err) => {
            fail_request(&connection, &request_id, &state, err.to_string()).await;
            return;
        }
    };

    let (Some(config), Some(grants), Some(stale)) =
        (crate::CONFIG.get(), crate::GRANTS.get(), crate::STALE.get())
    else {
        fail_request(
            &connection,
            &request_id,
            &state,
            "daemon not fully initialised".to_string(),
        )
        .await;
        return;
    };

    crate::finalize_completed_auth(&connection, config, &auth_manager, grants, stale, completed)
        .await;
}

/// Handler requests have no OAuth state to discard on timeout.
async fn terminal_timeout(
    request_id: String,
    state: Arc<Mutex<RequestState>>,
    connection: Connection,
) {
    tokio::time::sleep(REQUEST_TIMEOUT).await;
    if state.lock().await.status.is_terminal() {
        return;
    }
    fail_request(
        &connection,
        &request_id,
        &state,
        "The sign-in attempt timed out".to_string(),
    )
    .await;
}

async fn run_oauth_flow(
    provider_id: String,
    existing_account: Option<Uuid>,
    params: HashMap<String, String>,
    request_id: String,
    state: Arc<Mutex<RequestState>>,
    auth_manager: Arc<Mutex<AuthManager>>,
    connection: Connection,
) {
    let url_result = auth_manager
        .lock()
        .await
        .start_auth_flow(provider_id, request_id.clone(), existing_account, params)
        .await;

    let csrf_token = match url_result {
        Ok((url, csrf_token)) => {
            {
                let mut state = state.lock().await;
                state.status = RequestStatus::NeedsInteraction;
                state.interaction_uri = url;
                state.csrf_token = Some(csrf_token.clone());
            }
            emit_status_changed(&connection, &request_id, RequestStatus::NeedsInteraction).await;
            csrf_token
        }
        Err(err) => {
            tracing::error!("Failed to start authentication flow: {err}");
            fail_request(&connection, &request_id, &state, err.to_string()).await;
            return;
        }
    };

    tokio::spawn(request_timeout(
        request_id,
        csrf_token,
        state,
        auth_manager,
        connection,
    ));
}

async fn request_timeout(
    request_id: String,
    csrf_token: String,
    state: Arc<Mutex<RequestState>>,
    auth_manager: Arc<Mutex<AuthManager>>,
    connection: Connection,
) {
    tokio::time::sleep(REQUEST_TIMEOUT).await;

    let already_terminal = state.lock().await.status.is_terminal();
    if already_terminal {
        return;
    }

    auth_manager.lock().await.discard_pending(&csrf_token);
    fail_request(
        &connection,
        &request_id,
        &state,
        "The sign-in attempt timed out".to_string(),
    )
    .await;
}

pub(crate) async fn fail_request(
    connection: &Connection,
    request_id: &str,
    state: &Arc<Mutex<RequestState>>,
    reason: String,
) {
    {
        let mut state = state.lock().await;
        if state.status.is_terminal() {
            return;
        }
        state.status = RequestStatus::Failed;
        state.error_message = reason;
    }
    emit_status_changed(connection, request_id, RequestStatus::Failed).await;
    schedule_cleanup(connection.clone(), request_id.to_string());
}

pub(crate) async fn succeed_request(
    connection: &Connection,
    request_id: &str,
    state: &Arc<Mutex<RequestState>>,
    account_path: OwnedObjectPath,
) {
    {
        let mut state = state.lock().await;
        state.status = RequestStatus::Succeeded;
        state.account = account_path;
    }
    emit_status_changed(connection, request_id, RequestStatus::Succeeded).await;
    schedule_cleanup(connection.clone(), request_id.to_string());
}

async fn emit_status_changed(connection: &Connection, request_id: &str, status: RequestStatus) {
    let path = request_object_path(request_id);
    if let Ok(iface_ref) = connection
        .object_server()
        .interface::<_, RequestInterface>(path)
        .await
    {
        let _ = RequestInterface::request_status_changed(
            iface_ref.signal_emitter(),
            status.as_str().to_string(),
        )
        .await;
    }
}

fn schedule_cleanup(connection: Connection, request_id: String) {
    tokio::spawn(async move {
        tokio::time::sleep(REQUEST_GRACE_PERIOD).await;
        let path = request_object_path(&request_id);
        let _ = connection
            .object_server()
            .remove::<RequestInterface, _>(path)
            .await;
        if let Some(requests) = crate::REQUESTS.get() {
            requests.lock().await.remove(&request_id);
        }
    });
}

#[cfg(test)]
mod mail_tests {
    use super::*;

    fn settings() -> CustomMailSettings {
        CustomMailSettings {
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_starttls: true,
        }
    }

    #[test]
    fn custom_caldav_accounts_are_calendar_only() {
        let account = custom_caldav_account("https://dav.example.org/calendars/me/", "me").unwrap();
        assert_eq!(account.provider, CUSTOM_CALDAV_PROVIDER);
        assert_eq!(account.username, "me");
        assert_eq!(account.services, [(Service::Calendar, true)].into());
        assert_eq!(
            account.caldav_url.as_deref(),
            Some("https://dav.example.org/calendars/me/")
        );
    }

    #[tokio::test]
    async fn publishes_custom_caldav_endpoint_over_dbus() {
        use accounts_core::proxy::CalendarProxy;

        let url = "https://dav.example.org/calendars/me/";
        let account = custom_caldav_account(url, "me").unwrap();
        let path = account_object_path(&account.id);
        let server = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(path.clone(), crate::services::CalendarService::new(account))
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = Connection::session().await.unwrap();
        let calendar = CalendarProxy::builder(&client)
            .destination(server.unique_name().unwrap().clone())
            .unwrap()
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();

        assert_eq!(calendar.uri().await.unwrap(), url);
        assert_eq!(calendar.auth_method().await.unwrap(), "basic");
    }

    #[test]
    fn caldav_requires_an_https_url_without_embedded_secrets() {
        for url in [
            "http://dav.example.org/",
            "https://me:password@dav.example.org/",
            "https://dav.example.org/?token=fixture",
            "https://dav.example.org/#fragment",
        ] {
            assert!(custom_caldav_account(url, "me").is_err(), "{url}");
        }
        assert!(custom_caldav_account("https://dav.example.org/", "").is_err());
        assert!(custom_caldav_account("https://dav.example.org/", "me:other").is_err());
    }

    fn contacts_settings(protocol: &str, uri: &str, base_dn: &str) -> ContactsConnectionSettings {
        ContactsConnectionSettings {
            protocol: protocol.into(),
            uri: uri.into(),
            base_dn: base_dn.into(),
        }
    }

    #[test]
    fn custom_contacts_require_their_protocols_secure_url() {
        for settings in [
            contacts_settings("carddav", "http://dav.example.org", ""),
            contacts_settings("carddav", "https://user:password@dav.example.org", ""),
            contacts_settings("carddav", "https://dav.example.org", "dc=example,dc=org"),
            contacts_settings("ldap", "ldap://directory.example.org", "dc=example,dc=org"),
            contacts_settings("ldap", "ldaps://directory.example.org", ""),
            contacts_settings(
                "ldap",
                "ldaps://directory.example.org/dc=example,dc=org",
                "dc=example,dc=org",
            ),
            contacts_settings(
                "unknown",
                "ldaps://directory.example.org",
                "dc=example,dc=org",
            ),
        ] {
            assert!(custom_contacts_account(settings, "user").is_err());
        }
    }

    #[tokio::test]
    async fn contacts_dbus_exposes_carddav_and_ldap_settings() {
        use accounts_core::proxy::ContactsProxy;

        let cases = [
            (
                contacts_settings("carddav", "https://dav.example.org/addressbooks/user/", ""),
                "user",
                CUSTOM_CARDDAV_PROVIDER,
                "basic",
            ),
            (
                contacts_settings(
                    "ldap",
                    "ldaps://directory.example.org:636",
                    "dc=example,dc=org",
                ),
                "cn=reader,dc=example,dc=org",
                CUSTOM_LDAP_PROVIDER,
                "simple-bind",
            ),
        ];
        for (settings, username, provider, auth_method) in cases {
            let account = custom_contacts_account(settings.clone(), username).unwrap();
            assert_eq!(account.provider, provider);
            assert_eq!(account.username, username);
            assert_eq!(account.services, [(Service::Contacts, true)].into());
            let path = account_object_path(&account.id);
            let server = zbus::connection::Builder::session()
                .unwrap()
                .serve_at(path.clone(), crate::services::ContactsService::new(account))
                .unwrap()
                .build()
                .await
                .unwrap();
            let client = Connection::session().await.unwrap();
            let contacts = ContactsProxy::builder(&client)
                .destination(server.unique_name().unwrap().clone())
                .unwrap()
                .path(path)
                .unwrap()
                .build()
                .await
                .unwrap();

            assert_eq!(contacts.uri().await.unwrap(), settings.uri);
            assert_eq!(contacts.get_connection_settings().await.unwrap(), settings);
            assert_eq!(contacts.auth_method().await.unwrap(), auth_method);
        }
    }

    #[test]
    fn custom_mail_settings_are_email_only() {
        let account = custom_mail_account("person@example.com", "", settings()).unwrap();
        assert_eq!(account.provider, CUSTOM_MAIL_PROVIDER);
        assert_eq!(account.username, "person@example.com");
        assert_eq!(account.services, [(Service::Email, true)].into());
        let endpoints = crate::services::mail_settings(&account).unwrap();
        assert_eq!(endpoints.imap_tls, "tls");
        assert_eq!(endpoints.smtp_tls, "starttls");
        assert_eq!(endpoints.authentication, "password");
    }

    #[test]
    fn rejects_urls_and_invalid_connection_fields() {
        let mut bad = settings();
        bad.imap_host = "https://imap.example.com".into();
        assert!(custom_mail_account("person@example.com", "", bad).is_err());
        let mut bad = settings();
        bad.smtp_port = 0;
        assert!(custom_mail_account("person@example.com", "", bad).is_err());
        assert!(custom_mail_account("no-address", "", settings()).is_err());
        assert!(validate_password("bad\r\n").is_err());
    }
}

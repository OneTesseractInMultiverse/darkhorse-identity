use super::*;
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Input {
    CreateApplication {
        application: ApplicationInput,
    },
    UpdateApplication {
        application_id: String,
        revision: u64,
        application: ApplicationInput,
    },
    CreateResource {
        application_id: String,
        name: String,
    },
    CreateScope {
        application_id: String,
        resource_id: String,
        name: String,
    },
    CreateClient {
        application_id: String,
        client: ClientInput,
    },
    UpdateClient {
        application_id: String,
        client_id: String,
        revision: u64,
        client: ClientInput,
    },
    RotateSecret {
        application_id: String,
        client_id: String,
        revision: u64,
        overlap_seconds: u16,
    },
    RetireSecret {
        application_id: String,
        client_id: String,
        secret_id: String,
        revision: u64,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApplicationInput {
    name: String,
    owner_id: String,
    active: bool,
}
impl ApplicationInput {
    fn spec(self) -> Result<ApplicationSpec, RegistrationError> {
        Ok(ApplicationSpec {
            name: Label::new(&self.name)?,
            owner: id(&self.owner_id, PrincipalId::from_u128)?,
            active: self.active,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClientInput {
    name: String,
    active: bool,
    redirect_uris: Vec<String>,
    resource_ids: Vec<String>,
    scope_ids: Vec<String>,
    token_endpoint_auth_method: String,
}
impl ClientInput {
    fn spec(self) -> Result<ClientSpec, RegistrationError> {
        ClientSpec::new(
            Label::new(&self.name)?,
            self.active,
            crate::registration::redirects(self.redirect_uris)?,
            self.resource_ids
                .iter()
                .map(|s| id(s, ResourceId::from_u128))
                .collect::<Result<_, _>>()?,
            self.scope_ids
                .iter()
                .map(|s| id(s, ScopeId::from_u128))
                .collect::<Result<_, _>>()?,
            &self.token_endpoint_auth_method,
        )
    }
}
impl Input {
    pub(super) fn command(self) -> Result<Command, RegistrationError> {
        Ok(match self {
            Self::CreateApplication { application } => {
                Command::CreateApplication(application.spec()?)
            }
            Self::UpdateApplication {
                application_id,
                revision,
                application,
            } => Command::UpdateApplication {
                application: id(&application_id, ApplicationId::from_u128)?,
                revision,
                spec: application.spec()?,
            },
            Self::CreateResource {
                application_id,
                name,
            } => Command::CreateResource {
                application: id(&application_id, ApplicationId::from_u128)?,
                name: Label::new(&name)?,
            },
            Self::CreateScope {
                application_id,
                resource_id,
                name,
            } => Command::CreateScope {
                application: id(&application_id, ApplicationId::from_u128)?,
                resource: id(&resource_id, ResourceId::from_u128)?,
                name: ScopeName::new(&name)?,
            },
            Self::CreateClient {
                application_id,
                client,
            } => Command::CreateClient {
                application: id(&application_id, ApplicationId::from_u128)?,
                spec: client.spec()?,
            },
            Self::UpdateClient {
                application_id,
                client_id,
                revision,
                client,
            } => Command::UpdateClient {
                application: id(&application_id, ApplicationId::from_u128)?,
                client: id(&client_id, ClientId::from_u128)?,
                revision,
                spec: client.spec()?,
            },
            Self::RotateSecret {
                application_id,
                client_id,
                revision,
                overlap_seconds,
            } => Command::RotateSecret {
                application: id(&application_id, ApplicationId::from_u128)?,
                client: id(&client_id, ClientId::from_u128)?,
                revision,
                overlap_seconds,
            },
            Self::RetireSecret {
                application_id,
                client_id,
                secret_id,
                revision,
            } => Command::RetireSecret {
                application: id(&application_id, ApplicationId::from_u128)?,
                client: id(&client_id, ClientId::from_u128)?,
                secret: id(&secret_id, ClientSecretId::from_u128)?,
                revision,
            },
        })
    }
}
pub(super) fn id<T, E>(
    value: &str,
    constructor: impl FnOnce(u128) -> Result<T, E>,
) -> Result<T, RegistrationError> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| RegistrationError::Invalid)?;
    if uuid.to_string() != value {
        return Err(RegistrationError::Invalid);
    }
    constructor(uuid.as_u128()).map_err(|_| RegistrationError::Invalid)
}

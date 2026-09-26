//! Trusted, compiled presentation text. Protocol values and JSON diagnostics are never translated.
pub(in crate::operator) fn spanish(english: &str) -> Option<&'static str> {
    Some(match english {
        "Invalid command or arguments; use --help. Secret arguments are not accepted." => {
            "Comando o argumentos no válidos; use --help. No se aceptan secretos como argumentos."
        }
        "Operation not confirmed. Review the command and supply --yes for noninteractive execution." => {
            "Operación sin confirmar. Revise el comando e incluya --yes para ejecutarlo sin interacción."
        }
        "Operator command interrupted. The operation may have committed; inspect current state before retrying." => {
            "Comando interrumpido. La operación puede haberse confirmado; consulte el estado actual antes de reintentar."
        }
        "Cannot render or write command output. The operation may have committed; inspect current state before retrying." => {
            "No se puede generar o escribir la salida. La operación puede haberse confirmado; consulte el estado actual antes de reintentar."
        }
        "Invalid operator language configuration; use en or es." => {
            "Configuración de idioma no válida; use en o es."
        }
        "Darkhorse identity server and deployment-credential operator tools." => {
            "Servidor de identidad Darkhorse y herramientas administrativas con credenciales del despliegue."
        }
        "Language for human output; does not change command names or JSON records." => {
            "Idioma de los mensajes; no cambia los nombres de comandos ni los registros JSON."
        }
        "Confirm a mutation; does not grant authority or bypass safety checks." => {
            "Confirmar una modificación; no concede autoridad ni omite las comprobaciones de seguridad."
        }
        "Output is always uncolored." => "La salida nunca utiliza colores.",
        "Read administrator email/password and an optional mutation reason as protected JSON." => {
            "Leer el correo, la contraseña del administrador y un motivo opcional como JSON protegido."
        }
        "Account authentication input is too long." => {
            "Los datos de autenticación de la cuenta son demasiado largos."
        }
        "Account operation unavailable; check login configuration, database, limiter and audit availability." => {
            "Operación de cuenta no disponible; revise la configuración de acceso, la base de datos, el limitador y la auditoría."
        }
        "Account operation violates a directory invariant." => {
            "La operación de cuenta incumple una restricción del directorio."
        }
        "Administrator authentication or authority denied." => {
            "Autenticación o autoridad administrativa denegada."
        }
        "Administrator bootstrap is already complete." => {
            "La inicialización del administrador ya está completa."
        }
        "Administrator email: " => "Correo del administrador: ",
        "Administrator password: " => "Contraseña del administrador: ",
        "Application not found." => "No se encontró la aplicación.",
        "Application operation unavailable; check configuration, database, limiter and audit availability." => {
            "Operación de aplicación no disponible; revise la configuración, la base de datos, el limitador y la auditoría."
        }
        "Authentication attempt limit reached; wait before retrying." => {
            "Se alcanzó el límite de intentos de autenticación; espere antes de reintentar."
        }
        "Bootstrap could not be completed; verify database state before retrying." => {
            "No se pudo completar la inicialización; compruebe el estado de la base de datos antes de reintentar."
        }
        "Bootstrap input is too long." => "Los datos de inicialización son demasiado largos.",
        "Cannot access the foreground terminal." => {
            "No se puede acceder al terminal en primer plano."
        }
        "Cannot generate operation correlation." => {
            "No se puede generar el identificador de correlación de la operación."
        }
        "Cannot prepare Redis connections." => "No se pueden preparar las conexiones a Redis.",
        "Cannot prepare credentials." => "No se pueden preparar las credenciales.",
        "Cannot prepare terminal cancellation." => {
            "No se puede preparar la cancelación del terminal."
        }
        "Cannot read account authentication input." => {
            "No se pueden leer los datos de autenticación de la cuenta."
        }
        "Cannot read bootstrap input." => "No se pueden leer los datos de inicialización.",
        "Cannot read client configuration input." => {
            "No se puede leer la configuración del cliente."
        }
        "Cannot read terminal input." => "No se puede leer la entrada del terminal.",
        "Cannot restore terminal settings; restore them before entering more input." => {
            "No se puede restaurar el terminal; restaure su configuración antes de introducir más datos."
        }
        "Cannot write prompt." => "No se puede escribir la solicitud de entrada.",
        "Catalog read unavailable; check input, login configuration, database, limiter and audit availability." => {
            "Lectura del catálogo no disponible; revise los datos, la configuración de acceso, la base de datos, el limitador y la auditoría."
        }
        "Catalog target not found." => "No se encontró el elemento del catálogo.",
        "Client configuration input is too long." => {
            "La configuración del cliente es demasiado larga."
        }
        "Client not found in the requested application." => {
            "No se encontró el cliente en la aplicación indicada."
        }
        "Client operation unavailable; check configuration, database, limiter and audit availability." => {
            "Operación de cliente no disponible; revise la configuración, la base de datos, el limitador y la auditoría."
        }
        "Client or unretired secret not found in the requested scope." => {
            "No se encontró el cliente o un secreto sin retirar dentro del ámbito indicado."
        }
        "Client secret operation unavailable; check configuration, database, limiter and audit availability." => {
            "Operación de secreto de cliente no disponible; revise la configuración, la base de datos, el limitador y la auditoría."
        }
        "Confirm this operator mutation by typing yes: " => {
            "Confirme esta modificación administrativa escribiendo yes: "
        }
        "Database operation failed; verify configuration and database state." => {
            "Falló la operación de base de datos; compruebe la configuración y el estado de la base de datos."
        }
        "Email: " => "Correo electrónico: ",
        "First name: " => "Nombre: ",
        "Interactive bootstrap requires a terminal; use --stdin for protected piped input." => {
            "La inicialización interactiva requiere un terminal; use --stdin para una entrada protegida mediante una tubería."
        }
        "Invalid Redis configuration; check DARKHORSE_REDIS_* settings." => {
            "Configuración de Redis no válida; revise DARKHORSE_REDIS_*."
        }
        "Invalid account authentication input." => "Datos de autenticación de cuenta no válidos.",
        "Invalid account request; mutations require a bounded reason without control characters." => {
            "Solicitud de cuenta no válida; las modificaciones requieren un motivo de longitud limitada y sin caracteres de control."
        }
        "Invalid application identifier." => "Identificador de aplicación no válido.",
        "Invalid application name." => "Nombre de aplicación no válido.",
        "Invalid application request or inactive/missing owner." => {
            "Solicitud de aplicación no válida, o propietario inactivo o inexistente."
        }
        "Invalid bootstrap input." => "Datos de inicialización no válidos.",
        "Invalid catalog continuation." => "Continuación del catálogo no válida.",
        "Invalid client configuration input." => "Datos de configuración del cliente no válidos.",
        "Invalid client configuration or resource/scope allowance." => {
            "Configuración del cliente o autorización de recursos y ámbitos no válida."
        }
        "Invalid client identifier." => "Identificador de cliente no válido.",
        "Invalid client secret identifier." => "Identificador de secreto de cliente no válido.",
        "Invalid client secret operation." => "Operación de secreto de cliente no válida.",
        "Invalid database configuration; check DARKHORSE_DATABASE_* settings." => {
            "Configuración de base de datos no válida; revise DARKHORSE_DATABASE_*."
        }
        "Invalid database operation." => "Operación de base de datos no válida.",
        "Invalid expected revision." => "Revisión esperada no válida.",
        "Invalid operation identifier." => "Identificador de operación no válido.",
        "Invalid principal identifier." => "Identificador de identidad no válido.",
        "Invalid profile or password; review the documented input limits." => {
            "Perfil o contraseña no válidos; revise los límites documentados."
        }
        "Invalid provider origin." => "Origen del proveedor no válido.",
        "Invalid signing key identifier." => "Identificador de clave de firma no válido.",
        "Invalid signing key material or lifecycle input." => {
            "Material de clave de firma o datos de ciclo de vida no válidos."
        }
        "Invalid terminal input." => "Entrada de terminal no válida.",
        "Last name: " => "Apellido: ",
        "Limiter activation journal unavailable; check operator configuration, schema and grants. Inspect state before any retry." => {
            "Registro de activación del limitador no disponible; revise la configuración administrativa, el esquema y los permisos. Consulte el estado antes de reintentar."
        }
        "Limiter activation outcome unknown; inspect the operation record and enforcement state before any retry." => {
            "Resultado de activación del limitador desconocido; consulte el registro de la operación y el estado del control antes de reintentar."
        }
        "Limiter activation record not found." => {
            "No se encontró el registro de activación del limitador."
        }
        "Limiter operation failed or enforcement is untrusted; inspect state and follow the recovery procedure." => {
            "Falló la operación del limitador o su control no es confiable; consulte el estado y siga el procedimiento de recuperación."
        }
        "Limiter recovery wait has not elapsed, or the generation is already active." => {
            "No ha transcurrido el período de recuperación del limitador, o la generación ya está activa."
        }
        "Migration history or embedded manifest is incompatible. Stop and review the database with the matching release." => {
            "El historial de migraciones o el manifiesto integrado es incompatible. Deténgase y revise la base de datos con la versión correspondiente."
        }
        "Migration outcome is uncertain. Inspect the operation ID before any further change; do not repeat the command." => {
            "El resultado de la migración es incierto. Consulte el identificador de operación antes de realizar otro cambio; no repita el comando."
        }
        "Migration storage or database-owner authority is unavailable. Inspect the operation and database before any further change." => {
            "El almacenamiento de migraciones o la autoridad del propietario de la base de datos no está disponible. Consulte la operación y la base de datos antes de realizar otro cambio."
        }
        "Outcome unknown; inspect authoritative audit and account state before retrying." => {
            "Resultado desconocido; consulte la auditoría autoritativa y el estado de la cuenta antes de reintentar."
        }
        "Outcome unknown; inspect the application mutation audit and current state before retrying. Do not repeat creation without reconciliation." => {
            "Resultado desconocido; consulte la auditoría de modificación y el estado actual de la aplicación antes de reintentar. No repita la creación sin reconciliar el resultado."
        }
        "Outcome unknown; inspect the catalog read audit before retrying." => {
            "Resultado desconocido; consulte la auditoría de lectura del catálogo antes de reintentar."
        }
        "Outcome unknown; inspect the client mutation audit and current configuration before deciding to retry." => {
            "Resultado desconocido; consulte la auditoría de modificación del cliente y su configuración actual antes de decidir si reintenta."
        }
        "Outcome unknown; inspect the client secret audit and current inventory before deciding to retry." => {
            "Resultado desconocido; consulte la auditoría del secreto de cliente y el inventario actual antes de decidir si reintenta."
        }
        "Password confirmation does not match." => "La confirmación de contraseña no coincide.",
        "Password: " => "Contraseña: ",
        "Principal not found." => "No se encontró la identidad.",
        "Provider must be explicitly enabled." => "El proveedor debe habilitarse explícitamente.",
        "Provider or signing key not found." => "No se encontró el proveedor o la clave de firma.",
        "Provider state changed, key capacity is full, or configuration conflicts; inspect signing-status." => {
            "Cambió el estado del proveedor, se alcanzó la capacidad de claves o existe un conflicto de configuración; consulte signing-status."
        }
        "Reason (no secrets): " => "Motivo (sin secretos): ",
        "Redis infrastructure check failed; inspect the per-role result." => {
            "Falló la comprobación de infraestructura Redis; consulte el resultado de cada función."
        }
        "Signing key publication or verification window has not elapsed." => {
            "No ha transcurrido el período de publicación o verificación de la clave de firma."
        }
        "Signing operation unavailable; inspect state before retrying." => {
            "Operación de firma no disponible; consulte el estado antes de reintentar."
        }
        "Signing outcome is uncertain. Inspect the operation ID before any further change; do not repeat the command." => {
            "El resultado de firma es incierto. Consulte el identificador de operación antes de realizar otro cambio; no repita el comando."
        }
        "Terminal input cancelled." => "Entrada de terminal cancelada.",
        "Terminal input is too long." => "La entrada del terminal es demasiado larga.",
        "The application changed; read its current revision before retrying." => {
            "La aplicación cambió; consulte su revisión actual antes de reintentar."
        }
        "The client changed; read its current revision before retrying." => {
            "El cliente cambió; consulte su revisión actual antes de reintentar."
        }
        "The principal changed; read its current revision before retrying." => {
            "La identidad cambió; consulte su revisión actual antes de reintentar."
        }
        "Use the HTTP composition root for serve." => {
            "Use el punto de composición HTTP para serve."
        }
        "\nConfirm password: " => "\nConfirme la contraseña: ",
        "Cannot initialize runtime." => "No se puede inicializar el entorno de ejecución.",
        "Invalid server configuration; check DARKHORSE_* settings." => {
            "Configuración del servidor no válida; revise DARKHORSE_*."
        }
        "Cannot bind HTTP listener; check host and port availability." => {
            "No se puede enlazar el servicio HTTP; compruebe la disponibilidad del host y del puerto."
        }
        "HTTP server stopped unexpectedly." => "El servidor HTTP se detuvo inesperadamente.",
        "Media maintenance stopped unexpectedly." => {
            "El mantenimiento de archivos multimedia se detuvo inesperadamente."
        }
        "Email maintenance stopped unexpectedly." => {
            "El mantenimiento del correo se detuvo inesperadamente."
        }
        "Credential maintenance stopped unexpectedly." => {
            "El mantenimiento de credenciales se detuvo inesperadamente."
        }
        "Cannot install benchmark profiling signal handler." => {
            "No se puede instalar el manejador de señales del perfil de rendimiento."
        }
        "Run the HTTP server (also the default with no arguments)." => {
            "Ejecutar el servidor HTTP; también es la acción predeterminada sin argumentos."
        }
        "Existing local tools using trusted deployment credentials; see docs/operator-authority.md." => {
            "Herramientas locales con credenciales confiables del despliegue; consulte docs/operator-authority.md."
        }
        "Apply embedded database migrations, or inspect an earlier operation, using the database owner." => {
            "Aplicar migraciones integradas o consultar una operación anterior con el propietario de la base de datos."
        }
        "Create the single initial administrator." => "Crear el único administrador inicial.",
        "Read bounded protected JSON from standard input." => {
            "Leer JSON protegido y de tamaño limitado desde la entrada estándar."
        }
        "Read one authenticated page; repeat explicitly with the returned cursor." => {
            "Leer una página autenticada; repita explícitamente con el cursor recibido."
        }
        "Inspect one signing operation using only the primary database." => {
            "Consultar una operación de firma utilizando solo la base de datos primaria."
        }
        "Inspect one activation record without contacting Redis or changing state." => {
            "Consultar un registro de activación sin contactar Redis ni cambiar el estado."
        }
        "Read recorded progress and current history without running migrations." => {
            "Leer el progreso registrado y el historial actual sin ejecutar migraciones."
        }
        "Create an application after fresh administrator authentication and confirmation." => {
            "Crear una aplicación tras una autenticación administrativa reciente y una confirmación."
        }
        "Replace the name, owner and activation status at an expected revision." => {
            "Reemplazar el nombre, el propietario y el estado de activación en una revisión esperada."
        }
        "Read one application's configuration after fresh administrator authentication." => {
            "Leer la configuración de una aplicación tras una autenticación administrativa reciente."
        }
        "Read one authenticated page of applications." => {
            "Leer una página autenticada de aplicaciones."
        }
        "Replace a scoped client's complete configuration from protected authentication/configuration JSON." => {
            "Reemplazar la configuración completa de un cliente dentro de su ámbito mediante JSON protegido de autenticación y configuración."
        }
        "Read one client's configuration within its application; excludes credentials." => {
            "Leer la configuración de un cliente dentro de su aplicación; excluye las credenciales."
        }
        "Read one authenticated page of clients belonging to an application." => {
            "Leer una página autenticada de clientes de una aplicación."
        }
        "Lifecycle filter for application/client/capability lists; rejected for other catalogs." => {
            "Filtro de ciclo de vida para listas de aplicaciones, clientes y capacidades; se rechaza en otros catálogos."
        }
        "Read one page of credential lifecycle metadata; never reads secret values or verifiers." => {
            "Leer una página de metadatos del ciclo de vida de credenciales; nunca lee secretos ni verificadores."
        }
        "Permanently retire one scoped secret at an expected client revision." => {
            "Retirar permanentemente un secreto dentro de su ámbito en una revisión esperada del cliente."
        }
        "Read one page within the explicitly selected application." => {
            "Leer una página dentro de la aplicación seleccionada explícitamente."
        }
        "Read application-bound definitions or explicitly select all definitions; neither implies a grant." => {
            "Leer las definiciones vinculadas a una aplicación o seleccionar explícitamente todas; ninguna opción concede acceso."
        }
        "Select definitions explicitly bound to this application." => {
            "Seleccionar las definiciones vinculadas explícitamente a esta aplicación."
        }
        "Include every bound and unbound definition." => {
            "Incluir todas las definiciones, vinculadas o sin vincular."
        }
        "Print help" => "Mostrar ayuda",
        "Print version" => "Mostrar versión",
        "Print this message or the help of the given subcommand(s)" => {
            "Mostrar este mensaje o la ayuda de los subcomandos indicados"
        }
        "Invalid login configuration." => "Configuración de acceso no válida.",
        "Invalid provider configuration." => "Configuración del proveedor no válida.",
        "Invalid email delivery configuration." => "Configuración de entrega de correo no válida.",
        "Provider and email verification require enabled password authentication." => {
            "El proveedor y la verificación de correo requieren autenticación mediante contraseña habilitada."
        }
        "Invalid database configuration." => "Configuración de base de datos no válida.",
        "Invalid Redis configuration." => "Configuración de Redis no válida.",
        "Authentication database unavailable." => "Base de datos de autenticación no disponible.",
        "Login limiter key does not match the deployment or storage is unavailable." => {
            "La clave del limitador de acceso no coincide con el despliegue o el almacenamiento no está disponible."
        }
        "Cannot initialize login limiter." => "No se puede inicializar el limitador de acceso.",
        "Provider issuer or wrapping key conflicts with persisted state." => {
            "El emisor del proveedor o la clave de cifrado entra en conflicto con el estado persistido."
        }
        "Cannot initialize TLS email delivery." => {
            "No se puede inicializar la entrega de correo mediante TLS."
        }
        "Email origin or key conflicts with persisted state, or storage is unavailable." => {
            "El origen o la clave del correo entra en conflicto con el estado persistido, o el almacenamiento no está disponible."
        }
        "Invalid object storage configuration." => {
            "Configuración de almacenamiento de objetos no válida."
        }
        "Object storage identity conflicts with persisted state or storage is unavailable." => {
            "La identidad del almacenamiento de objetos entra en conflicto con el estado persistido o el almacenamiento no está disponible."
        }
        "Invalid introspection admission configuration." => {
            "Configuración de admisión de introspección no válida."
        }
        "Cannot initialize introspection limiter." => {
            "No se puede inicializar el limitador de introspección."
        }
        _ => return None,
    })
}

use std::collections::BTreeSet;

use crate::{ErrorCode, IpcError, ProtocolVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NegotiatedProtocol {
    pub version: ProtocolVersion,
    pub capabilities: BTreeSet<String>,
}

/// Negotiates one major-compatible protocol and the capability intersection.
pub fn negotiate(
    client_version: ProtocolVersion,
    client_capabilities: &BTreeSet<String>,
    daemon_version: ProtocolVersion,
    daemon_capabilities: &BTreeSet<String>,
) -> Result<NegotiatedProtocol, IpcError> {
    if client_version.major != daemon_version.major {
        return Err(IpcError::new(
            ErrorCode::IncompatibleVersion,
            format!(
                "client protocol {}.{} is incompatible with daemon protocol {}.{}",
                client_version.major,
                client_version.minor,
                daemon_version.major,
                daemon_version.minor
            ),
            false,
        ));
    }

    let capabilities = client_capabilities
        .intersection(daemon_capabilities)
        .cloned()
        .collect();

    Ok(NegotiatedProtocol {
        version: ProtocolVersion::new(
            daemon_version.major,
            client_version.minor.min(daemon_version.minor),
        ),
        capabilities,
    })
}

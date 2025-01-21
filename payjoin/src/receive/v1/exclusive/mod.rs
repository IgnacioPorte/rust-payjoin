use std::str::FromStr;
mod error;
pub(crate) use error::InternalRequestError;
pub use error::RequestError;

use super::*;
use crate::receive::optional_parameters::Params;

const SUPPORTED_VERSIONS: &[usize] = &[1];

pub trait Headers {
    fn get_header(&self, key: &str) -> Option<&str>;
}

pub fn build_v1_pj_uri<'a>(
    address: &bitcoin::Address,
    endpoint: &url::Url,
    disable_output_substitution: bool,
) -> crate::uri::PjUri<'a> {
    let extras =
        crate::uri::PayjoinExtras { endpoint: endpoint.clone(), disable_output_substitution };
    bitcoin_uri::Uri::with_extras(address.clone(), extras)
}

impl UncheckedProposal {
    pub fn from_request(
        mut body: impl std::io::Read,
        query: &str,
        headers: impl Headers,
    ) -> Result<Self, Error> {
        let content_type = headers
            .get_header("content-type")
            .ok_or(InternalRequestError::MissingHeader("Content-Type"))?;
        if !content_type.starts_with("text/plain") {
            return Err(InternalRequestError::InvalidContentType(content_type.to_owned()).into());
        }
        let content_length = headers
            .get_header("content-length")
            .ok_or(InternalRequestError::MissingHeader("Content-Length"))?
            .parse::<u64>()
            .map_err(InternalRequestError::InvalidContentLength)?;
        // 4M block size limit with base64 encoding overhead => maximum reasonable size of content-length
        if content_length > 4_000_000 * 4 / 3 {
            return Err(InternalRequestError::ContentLengthTooLarge(content_length).into());
        }

        // enforce the limit
        let mut buf = vec![0; content_length as usize]; // 4_000_000 * 4 / 3 fits in u32
        body.read_exact(&mut buf).map_err(InternalRequestError::Io)?;
        let base64 = String::from_utf8(buf).map_err(InternalPayloadError::Utf8)?;
        let unchecked_psbt = Psbt::from_str(&base64).map_err(InternalPayloadError::ParsePsbt)?;

        let psbt = unchecked_psbt.validate().map_err(InternalPayloadError::InconsistentPsbt)?;
        log::debug!("Received original psbt: {:?}", psbt);

        let pairs = url::form_urlencoded::parse(query.as_bytes());
        let params = Params::from_query_pairs(pairs, SUPPORTED_VERSIONS)
            .map_err(InternalPayloadError::SenderParams)?;
        log::debug!("Received request with params: {:?}", params);

        // TODO check that params are valid for the request's Original PSBT

        Ok(UncheckedProposal { psbt, params })
    }
}

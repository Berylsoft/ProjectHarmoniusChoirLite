use axum::http::StatusCode;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use problem_details::ProblemDetails;
use serde::de::DeserializeOwned;
use signed_data::SignedData;

use crate::{BotKey, routes};

/// `additional_validation` should return true if valid false otherwise.
///
/// # Errors
///
/// not valid base64 or
/// signature doesn't match `key` or
/// failed to deserialize or
/// `additional_validation` returns invalid.
pub fn validate_bot_token<T, V>(
    token: &str,
    key: &BotKey,
    additional_validation: V,
) -> routes::Result<T>
where
    T: DeserializeOwned,
    V: FnOnce(&T) -> bool,
{
    let invalid_token = || {
        Err(ProblemDetails::from_status_code(StatusCode::UNAUTHORIZED)
            .with_detail("invalid token")
            .into())
    };

    let data = SignedData::<T>::try_from_base64(token, &URL_SAFE_NO_PAD);
    let data = match data {
        Ok(data) => data,
        Err(err) => {
            tracing::debug!("invalid token encoding: {err}");
            return invalid_token();
        }
    };

    let token = data.to_verified(key);
    let token = match token {
        Ok(token) => token,
        Err(err) => {
            tracing::debug!("invalid token: {err}");
            return invalid_token();
        }
    };

    if !additional_validation(&token) {
        return invalid_token();
    }

    Ok(token)
}

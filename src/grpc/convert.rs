use super::api::FilteredIpAddr;
use crate::Error;
use crate::Result as ApiResult;
use anyhow::anyhow;
use prost_types::Timestamp;

/// Function to convert the datetimes from the database into the API representation of a timestamp.
pub fn try_dt_to_ts(datetime: chrono::DateTime<chrono::Utc>) -> crate::Result<Timestamp> {
    const NANOS_PER_SEC: i64 = 1_000_000_000;
    let nanos = datetime.timestamp_nanos();
    let timestamp = Timestamp {
        seconds: nanos / NANOS_PER_SEC,
        // This _should_ never fail because 1_000_000_000 fits into an i32, but using `as` was
        // hiding a bug here at first, therefore I have left the `try_into` call here.
        nanos: (nanos % NANOS_PER_SEC).try_into()?,
    };
    Ok(timestamp)
}

pub fn json_value_to_vec(json: &serde_json::Value) -> ApiResult<Vec<FilteredIpAddr>> {
    let arr = json
        .as_array()
        .ok_or_else(|| Error::UnexpectedError(anyhow!("Error deserializing JSON object")))?;
    let mut result = vec![];

    for value in arr {
        let tmp = value
            .as_object()
            .ok_or_else(|| Error::UnexpectedError(anyhow!("Error deserializing JSON array")))?;
        let ip = tmp
            .get("ip")
            .map(|e| e.to_string())
            .ok_or_else(|| Error::UnexpectedError(anyhow!("Can't read IP")))?
            .to_string();
        let description = tmp.get("description").map(|e| e.to_string());

        result.push(FilteredIpAddr { ip, description });
    }

    Ok(result)
}

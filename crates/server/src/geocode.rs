use std::time::Duration;

/// Reverse-geocode a lat/lon pair using the Mapbox Geocoding API v6.
///
/// Returns a human-readable location string (e.g. "San Francisco, United States")
/// or `None` on any error. This function never panics and is designed to be
/// best-effort — upload should not fail because geocoding is unavailable.
pub async fn reverse_geocode(
    client: &reqwest::Client,
    token: &str,
    lat: f64,
    lon: f64,
) -> Option<String> {
    let url = format!(
        "https://api.mapbox.com/search/geocode/v6/reverse?longitude={lon}&latitude={lat}&access_token={token}&types=place,country"
    );

    let resp = tokio::time::timeout(Duration::from_secs(5), client.get(&url).send())
        .await
        .ok()?  // timeout
        .ok()?; // request error

    if !resp.status().is_success() {
        tracing::warn!(
            status = %resp.status(),
            "mapbox geocode request failed"
        );
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;

    // Try to extract a useful location name from the response.
    // The v6 API returns features sorted by relevance; the first one with
    // type "place" gives us the city, and "country" gives the country.
    let features = body.get("features")?.as_array()?;

    let mut place: Option<&str> = None;
    let mut country: Option<&str> = None;

    for feature in features {
        let props = feature.get("properties")?;
        let feat_type = props.get("feature_type").and_then(|v| v.as_str())?;
        match feat_type {
            "place" if place.is_none() => {
                place = props.get("name").and_then(|v| v.as_str());
            }
            "country" if country.is_none() => {
                country = props.get("name").and_then(|v| v.as_str());
            }
            _ => {}
        }
    }

    match (place, country) {
        (Some(p), Some(c)) => Some(format!("{}, {}", p, c)),
        (Some(p), None) => Some(p.to_string()),
        (None, Some(c)) => Some(c.to_string()),
        (None, None) => {
            // Fallback: try full_address from the first feature
            features
                .first()
                .and_then(|f| f.get("properties"))
                .and_then(|p| p.get("full_address"))
                .and_then(|v| v.as_str())
                .map(String::from)
        }
    }
}

use std::time::Duration;

/// Reverse-geocode a lat/lon pair using the Mapbox Geocoding API v6.
///
/// Returns a location string formatted as "City, Country [CC]" where CC is the
/// ISO 3166-1 alpha-2 country code (e.g. "San Francisco, United States [US]").
/// Returns `None` on any error. Best-effort — upload should not fail because
/// geocoding is unavailable.
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

    let features = body.get("features")?.as_array()?;

    let mut place: Option<&str> = None;
    let mut country: Option<&str> = None;
    let mut country_code: Option<String> = None;

    for feature in features {
        let props = match feature.get("properties") {
            Some(p) => p,
            None => continue,
        };
        let feat_type = match props.get("feature_type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        match feat_type {
            "place" if place.is_none() => {
                place = props.get("name").and_then(|v| v.as_str());
            }
            "country" if country.is_none() => {
                country = props.get("name").and_then(|v| v.as_str());
                country_code = props
                    .get("country_code")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_uppercase());
            }
            _ => {}
        }

        // Extract country info from context on any feature
        if country_code.is_none() || country.is_none() {
            if let Some(ctx) = props
                .get("context")
                .and_then(|c| c.get("country"))
            {
                if country_code.is_none() {
                    country_code = ctx
                        .get("country_code")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_uppercase());
                }
                if country.is_none() {
                    country = ctx.get("name").and_then(|v| v.as_str());
                }
            }
        }
    }

    let base = match (place, country) {
        (Some(p), Some(c)) => format!("{}, {}", p, c),
        (Some(p), None) => p.to_string(),
        (None, Some(c)) => c.to_string(),
        (None, None) => {
            features
                .first()
                .and_then(|f| f.get("properties"))
                .and_then(|p| p.get("full_address"))
                .and_then(|v| v.as_str())
                .map(String::from)?
        }
    };

    // Append country code if available: "City, Country [CC]"
    match country_code {
        Some(cc) => Some(format!("{} [{}]", base, cc)),
        None => Some(base),
    }
}

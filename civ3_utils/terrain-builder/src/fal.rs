//! fal.ai text-to-image client (synchronous `fal.run` endpoint).
//!
//! Auth: `FAL_KEY` in the environment, as the fal SDKs use.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

pub struct Request<'a> {
    pub model: &'a str,
    pub prompt: &'a str,
    pub image_size: &'a str,
    pub seed: Option<u64>,
}

pub fn api_key() -> Result<String> {
    std::env::var("FAL_KEY")
        .or_else(|_| std::env::var("FAL_API_KEY"))
        .map_err(|_| anyhow!("set FAL_KEY to a fal.ai API key (https://fal.ai/dashboard/keys)"))
}

/// Generate one image; returns the encoded image bytes.
pub fn generate(req: &Request) -> Result<Vec<u8>> {
    let key = api_key()?;
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(600)).build();
    let mut body = json!({
        "prompt": req.prompt,
        "image_size": req.image_size,
        "num_images": 1,
        "enable_safety_checker": true,
        "output_format": "png",
    });
    if let Some(seed) = req.seed {
        body["seed"] = json!(seed);
    }
    let url = format!("https://fal.run/{}", req.model);
    let resp: Value = match agent.post(&url).set("Authorization", &format!("Key {key}")).send_json(body) {
        Ok(r) => r.into_json().context("decoding fal.ai response")?,
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            bail!("fal.ai {} returned {code}: {text}", req.model);
        }
        Err(e) => return Err(e).context("calling fal.ai"),
    };
    let image_url = resp["images"][0]["url"]
        .as_str()
        .or_else(|| resp["image"]["url"].as_str())
        .ok_or_else(|| anyhow!("fal.ai response has no image url: {resp}"))?;
    if image_url.starts_with("data:") {
        bail!("fal.ai returned an inline data URI; disable sync_mode for this model");
    }
    let mut bytes = Vec::new();
    agent
        .get(image_url)
        .call()
        .with_context(|| format!("downloading {image_url}"))?
        .into_reader()
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

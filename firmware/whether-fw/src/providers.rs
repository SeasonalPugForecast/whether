//! The provider chain from the embedded configuration.

use alloc::boxed::Box;
use alloc::vec::Vec;

use fw_config::{FwOwmApi, FwProvider};
use whether_providers::Provider;
use whether_providers::met_norway::{MetNorway, MetNorwaySettings, MetProduct};
use whether_providers::open_meteo::{OpenMeteo, OpenMeteoSettings};
use whether_providers::openweather::{OpenWeather, OpenWeatherSettings};

/// Builds the providers for `configured`, in order.
pub fn build(configured: &[FwProvider]) -> Vec<Box<dyn Provider>> {
    configured.iter().map(provider).collect()
}

fn provider(p: &FwProvider) -> Box<dyn Provider> {
    match p {
        FwProvider::OpenWeather {
            api_key,
            api,
            daily_call_limit,
        } => Box::new(OpenWeather::new(match api {
            FwOwmApi::Free => OpenWeatherSettings::free(api_key.clone()),
            FwOwmApi::OneCall4 => OpenWeatherSettings::onecall4(api_key.clone(), *daily_call_limit),
        })),
        FwProvider::OpenMeteo { api_key } => Box::new(OpenMeteo::new(OpenMeteoSettings {
            api_key: api_key.clone(),
            ..OpenMeteoSettings::default()
        })),
        FwProvider::MetNorway { contact, compact } => Box::new(MetNorway::new(MetNorwaySettings {
            product: if *compact {
                MetProduct::Compact
            } else {
                MetProduct::Complete
            },
            ..MetNorwaySettings::new(contact.clone())
        })),
    }
}

# Privacy

*Last updated: 2 November 2026.* This policy is for the Pug Forecast iPhone app (the Whether
project, <https://github.com/SeasonalPugForecast/whether>).

Whether has no accounts, no ads, no analytics and no tracking. The project runs no server, and
nothing you do in the app is sent to it.

## What leaves your phone

To get a forecast, the app sends the **coordinates of each place you save** (and of the place a
trip plan is for), **to two decimals (about 1 km)**, and your language, to the weather services
below. Apple WeatherKit and Apple's place naming (reverse geocoding of your location) receive the
same rounded coordinates. The radar asks the radar service for the map tiles you are looking at,
so it sees roughly where you are looking. When you type in the place search, the text goes to
Apple Maps for suggestions and, if Apple has none, to Open-Meteo's place search; a search that is
only digits (a postal code) also sends your phone's region code. Those services see your phone's IP
address, as any web request does, and keep their own logs under their own policies.

None of the weather services receives your name, a device identifier or the names of your plans.
(A place's name can reach Apple Maps or Open-Meteo only as search text you type.) A service you
give your own key to (OpenWeather, Pirate Weather) can tie your requests to your account with it.

| Service | Used for |
|---|---|
| [Apple Maps](https://www.apple.com/legal/privacy/data/en/apple-maps/) | place suggestions as you type a search (the text typed, ranked near the place shown, which is sent to about 1 km), and naming the place your location is |
| [Open-Meteo](https://open-meteo.com/en/terms) | forecasts, history, place search when Apple has no suggestion |
| [MET Norway](https://api.met.no/doc/TermsOfService) | the backup forecast, only when Open-Meteo cannot answer and you leave it on; the request names the project's web address as its contact |
| [National Weather Service](https://www.weather.gov/privacy) | US alerts, and US forecasts if you switch them on |
| [NOAA Regional Climate Centers (ACIS)](https://www.rcc-acis.org/) | US climate normals and history |
| [Iowa Environmental Mesonet](https://mesonet.agron.iastate.edu/disclaimer.php) | radar tiles |
| [OpenWeather](https://openweathermap.org/privacy-policy) | only if you enter your own API key: forecasts, and the radar map outside the US (the request carries your key and the map tile wanted) |
| [Pirate Weather](https://pirateweather.net) | only if you enter your own API key: forecasts, the next hour of rain and alerts (the request carries your key, in a header, and the rounded coordinates; testing the key sends a request for 0°, 0° and counts as one of the key's calls) |
| [Apple WeatherKit](https://developer.apple.com/weatherkit/data-source-attribution/) | the next hour minute by minute and, outside the US, weather alerts, if you leave them on |
| [Apple iCloud](https://www.apple.com/legal/privacy/en-ww/) | only for plans you share live, and to look for ones you shared or accepted on another device (see "Sharing a plan live through iCloud") |

"Current location" reads your location once to name and save the place. If you choose a place that
follows you ("Use my location" on the welcome screen, or "Follow me" in Places), the app reads your
location each time it opens, with the "while using the app" permission only (never in the
background, and the widget uses the last saved coordinates); only the rounded coordinates leave the
phone, as for any place. The reading itself stays on the phone.

## Notifications, widgets and background refresh

- **Notifications** (weather alerts and shared-plan changes) are made on your phone after the app
  has fetched or synced. The project sends none, and you can turn them off in iOS Settings.
- **Background refresh and widgets** make the same requests as the app, for the same rounded
  coordinates, when iOS lets the app refresh or a widget's data is old.
- **The rain Live Activity** is updated by the app on your phone only.

## What stays on your phone

Your places (with their full coordinates), trip plans, settings and cached forecasts are stored on
the phone and shared with the app's widget through an App Group. Like other app data, they can be
part of your iPhone backups, under your own backup settings with Apple. A key for a weather source
(such as OpenWeather), if you enter one, is kept in the iOS Keychain on this phone only, one item per
source; it is not synced to iCloud and never appears in logs.

## Sharing a plan live through iCloud

You can share a trip plan so that the people you invite see and, if you allow it, change it. Only
plans you choose to share go to iCloud; your other plans stay on the phone.

- **What is stored.** The plan's name (also the title of the invitation), its place (name, region,
  coordinates as saved and time zone), its dates and years, a random id, the time of the last edit
  and the name of whoever made it, as iCloud gives that name (or "Someone").
- **Where.** In your own iCloud account, in the app's iCloud container, under your Apple Account
  and [Apple's terms](https://www.apple.com/legal/privacy/en-ww/). The project cannot read it.
- **Who sees it.** The people you invite, who get the invitation through the iOS share sheet
  (Messages, Mail or another app you choose, which sees the invitation). They see the plan's
  contents above, and you and they see each other's names as iCloud shows them.
- **What the app asks iCloud.** Once each time it opens, where iCloud is available, the app looks
  for plans you shared or accepted on another device. This is a request to Apple, not to the
  project, and it happens even if you never shared a plan. Once you share or accept one, the app
  asks iCloud to tell it when a plan changes: iCloud then sends the phone silent notifications
  through Apple's push service, which uses a token for your phone that goes to Apple only.
- **Stopping.** "Stop sharing" deletes the shared copy from iCloud and everyone's access to it;
  "Leave" removes you from a plan someone shared. The plan stays on your phone as a private one. You
  can also turn iCloud off for the app in iOS Settings.

## Sharing a plan or colors as a file

When you tap Share on a trip plan and send a copy, the app makes a `.whetherplan` file and hands it
to the iOS share sheet. The file holds the plan's name, its place (name, region, coordinates as
saved and time zone), its dates and years, a random id for the plan, and the time it was made. It
holds no location history, no other places, no keys and no settings. It goes only where you send it
(Messages, Mail, AirDrop or Files), through that service, and the project never sees it. When you
open a plan someone sent, the app shows it with Add and Decline, and nothing is saved until you
choose Add.

When you tap Share on one of your own color themes, the app makes a `.whethertheme` file and hands
it to the iOS share sheet. The file holds the theme's name, the preset it was made from, the colors
you changed and whether patterns are always on. It holds no place, no settings, no keys and no id.
It goes only where you send it, through that service, and the project never sees it. When you open
a theme someone sent, the app shows a sample with Add and Decline, and nothing is saved until you
choose Add. Color themes stay on the phone; they are not synced through iCloud.

## Deleting your data

Deleting the app removes your places, plans, settings and cached forecasts from the phone. Two
things can outlast it, so clear them first if you want them gone:

- **API keys** in the Keychain: iOS may keep them after the app is deleted. Clear the key field in
  Settings → Sources before deleting.
- **Plans shared live** stay in iCloud until you stop sharing them (or leave them).

## Children

The app is not directed at children and collects no personal information from anyone.

## Changes

The date at the top changes with every edit. The history of this file is public in the project
repository.

## Questions

Open an issue in the project repository: <https://github.com/SeasonalPugForecast/whether/issues>, or
write to <pugforecast@gmail.com>. Please leave API keys and your exact address out of both.

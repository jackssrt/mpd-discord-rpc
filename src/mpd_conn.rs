use crate::config::TimestampMode;
use discord_presence::models::ActivityTimestamps;
use mpd_client::responses::{Song, Status};
use mpd_client::tag::Tag;
use std::borrow::Cow;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returned in place of a token which has no value
const UNKNOWN: &str = "N/A";

/// Formats a duration given in seconds
/// in hh:mm format
fn format_time(time: u64) -> String {
    let minutes = (time / 60) % 60;
    let seconds = time % 60;

    format!("{minutes:0>2}:{seconds:0>2}")
}

/// Converts a string format token value
/// into its respective MPD value.
pub fn get_token_value(song: &Song, status: &Status, token: &str) -> String {
    let value = match token {
        "title" => song.title(),
        "album" => try_get_first_tag(song.tags.get(&Tag::Album)),
        "artist" => try_get_first_tag(song.tags.get(&Tag::Artist)),
        "albumartist" => try_get_first_tag(song.tags.get(&Tag::AlbumArtist)),
        "date" => try_get_first_tag(song.tags.get(&Tag::Date)),
        "disc" => try_get_first_tag(song.tags.get(&Tag::Disc)),
        "genre" => try_get_first_tag(song.tags.get(&Tag::Genre)),
        "track" => try_get_first_tag(song.tags.get(&Tag::Track)),
        "originaldate" => try_get_first_tag(song.tags.get(&Tag::OriginalDate)),
        "duration" => return get_duration(status).map_or_else(|| String::from("N/A"), format_time),
        "elapsed" => return get_elapsed(status).map_or_else(|| String::from("N/A"), format_time),
        _ => Some(token),
    };

    match value {
        Some(value) if is_multi_value_tag(token) => match format_multi_value(value) {
            formatted if formatted.is_empty() => UNKNOWN.to_string(),
            formatted => formatted.into_owned(),
        },
        Some(value) => value.to_string(),
        None => UNKNOWN.to_string(),
    }
}

/// Returns whether the given token can contain multiple values,
/// separated by semicolons
fn is_multi_value_tag(token: &str) -> bool {
    matches!(token, "artist" | "albumartist" | "genre")
}

/// Formats a tag containing a semicolon delimited list of values
/// (e.g. multiple artists) into a human readable string.
///
/// Each separator is replaced with a comma, except for the last one,
/// which is replaced with an ampersand. Whitespace surrounding each
/// separator is trimmed and normalised to a single space on both sides.
/// Empty values are dropped.
fn format_multi_value(value: &str) -> Cow<'_, str> {
    if !value.contains(';') {
        return Cow::Borrowed(value);
    }

    let values = value
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let Some((last, rest)) = values.split_last() else {
        return Cow::Borrowed("");
    };

    if rest.is_empty() {
        return Cow::Owned(last.to_string());
    }

    Cow::Owned(format!("{} & {last}", rest.join(", ")))
}

/// Gets the activity timestamp based off the current song elapsed/remaining
pub fn get_timestamp(status: &Status, mode: TimestampMode) -> ActivityTimestamps {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Failed to get system time")
        .as_secs();

    let timestamps = ActivityTimestamps::new();

    let Some(elapsed) = get_elapsed(status) else {
        return timestamps;
    };

    match mode {
        TimestampMode::Left => {
            let Some(duration) = get_duration(status) else {
                return timestamps;
            };

            let remaining = duration - elapsed;
            timestamps.end(current_time + remaining)
        }
        TimestampMode::Off => timestamps,
        TimestampMode::Elapsed => timestamps.start(current_time - elapsed),
        TimestampMode::Both => {
            let Some(duration) = get_duration(status) else {
                return timestamps;
            };
            let start_timestamp = current_time - elapsed;
            let end_timestamp = start_timestamp + duration;
            timestamps.start(start_timestamp).end(end_timestamp)
        }
    }
}

/// Attempts to read the first value for a tag
/// (since the MPD client returns a vector of tags, or None)
pub fn try_get_first_tag(vec: Option<&Vec<String>>) -> Option<&str> {
    vec.and_then(|vec| vec.first().map(String::as_str))
}

/// Gets the duration of the current song
fn get_duration(status: &Status) -> Option<u64> {
    status.duration.map(|d| d.as_secs())
}

/// Gets the elapsed time of the current song
fn get_elapsed(status: &Status) -> Option<u64> {
    status.elapsed.map(|e| e.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_value_is_unchanged() {
        assert_eq!(format_multi_value("Aphex Twin"), "Aphex Twin");
    }

    #[test]
    fn two_values_are_joined_with_an_ampersand() {
        assert_eq!(
            format_multi_value("Autechre;Boards of Canada"),
            "Autechre & Boards of Canada"
        );
    }

    #[test]
    fn only_the_last_separator_is_an_ampersand() {
        assert_eq!(format_multi_value("a;b;c;d"), "a, b, c & d");
    }

    #[test]
    fn existing_surrounding_whitespace_is_normalised() {
        assert_eq!(format_multi_value("a ;b; c;d "), "a, b, c & d");
        assert_eq!(format_multi_value("a;b"), "a & b");
    }

    #[test]
    fn empty_values_are_dropped() {
        assert_eq!(format_multi_value(";a;;b;"), "a & b");
        assert_eq!(format_multi_value(";;"), "");
    }
}

use bytes::BufMut;
use eyre::eyre as err;
use regex::Regex;
use std::str::{from_utf8, FromStr};
use std::sync::LazyLock;

use bilrost::Canonicity::{Canonical, HasExtensions, NotCanonical};
use bilrost::{
    DecodeError, DecodeErrorKind, DistinguishedOwnedMessage, OwnedMessage, WithCanonicity,
};

pub mod test_messages;

pub fn test_message(data: &[u8]) {
    let _ = roundtrip::<test_messages::TestAllTypes>(data).unwrap_error();
    let _ = roundtrip_distinguished::<test_messages::TestDistinguished>(data).unwrap_error();
}

pub fn test_type_support(data: &[u8]) {
    let _ = roundtrip::<test_messages::TestTypeSupport>(data).unwrap_error();
    let _ =
        roundtrip_distinguished::<test_messages::TestTypeSupportDistinguished>(data).unwrap_error();
}

static DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    // exactly four digits, or +/- followed by any number
    let year = r"(\d{4}|[+-]\d+)";
    // 01-12
    let month = r"(0[1-9]|1[0-2])";
    // 01-31
    let day_of_month = r"(0[1-9]|[1-2]\d|3[01])";
    // date components are always separated by '-'
    let date = format!("{year}-{month}-{day_of_month}");
    // 00-23
    let hour = r"([01]\d|2[0-3])";
    // 00-59
    let minute = r"[0-5]\d";
    // 00-60
    let second = r"([0-5]\d|60)";
    // decimal point with 1-9 digits following
    let fraction = r"\.\d+";
    // time always contains hour, minute, and second; fraction is optional
    let time = format!("{hour}:{minute}:{second}({fraction})?");
    // offset is +/- 00-99 hours and (optionally) 00-99 minutes, with an optional ':' separating
    let offset = r"[+-]\d\d(:?\d\d)?";
    // the letter Z is also an acceptable offset, indicating no offset (UTC)
    let offset_or_zulu = format!("({offset}|[Zz])");
    // may contain date, date & time, or date & time & offset. time is separated by a space or the
    // letter T, and timezone offset may be separated from the time by an optional space.
    Regex::new(&format!("^{date}([ Tt]{time}( ?{offset_or_zulu})?)?$")).unwrap()
});

pub fn test_parse_date(data: &[u8]) {
    // input must be text
    let Ok(original_text) = from_utf8(data) else {
        return;
    };

    // parse input as a datetime
    let Ok(timestamp) = bilrost_types::Timestamp::from_str(original_text) else {
        let chrono_parse = chrono::DateTime::parse_from_rfc3339(original_text);
        assert!(
            chrono_parse.is_err(),
            "bilrost_types failed to parse time, but chrono does parse this time: {}",
            original_text
        );
        return;
    };

    // check that it matches our regex pattern
    assert!(DATE_RE.is_match(original_text));

    // roundtrip to and from string
    let roundtrip_text = format!("{timestamp}");
    assert_eq!(Ok(&timestamp), roundtrip_text.parse().as_ref());

    // chrono can only parse year 0000 till 9999
    let Ok(chrono_time) = chrono::DateTime::parse_from_rfc3339(original_text) else {
        return;
    };
    if chrono_time.timestamp_subsec_nanos() > 999_999_999 {
        // bilrost_types ignores leap seconds, but chrono increases the nanos in that case
        return;
    }

    assert_eq!(timestamp.seconds, chrono_time.timestamp());
    assert_eq!(timestamp.nanos, chrono_time.timestamp_subsec_nanos() as i32);

    // roundtrip using chrono
    let chrono_text = chrono_time.to_utc().to_rfc3339();
    assert_eq!(
        roundtrip_text.strip_suffix("Z"),
        chrono_text.strip_suffix("+00:00")
    );
}

pub fn test_parse_duration(data: &[u8]) {
    use std::str::from_utf8;
    use std::str::FromStr;

    // input must be text
    let Ok(original_text) = from_utf8(data) else {
        return;
    };

    // parse input as a duration
    let Ok(duration) = bilrost_types::Duration::from_str(original_text) else {
        if original_text.ends_with("s") {
            assert!(
                original_text.parse::<f64>().is_err(),
                "bilrost failed to parse duration, but it seems to be a valid number: {}",
                original_text
            );
        }
        return;
    };

    // roundtrip to and from string
    let roundtrip_text = format!("{duration}");
    assert_eq!(Ok(&duration), roundtrip_text.parse().as_ref());
}

enum RoundtripResult {
    /// The roundtrip succeeded.
    Ok(Vec<u8>),
    /// The data could not be decoded. This could indicate a bug in bilrost,
    /// or it could indicate that the input was bogus.
    DecodeError(DecodeError),
    /// Re-encoding or validating the data failed.  This indicates a bug in `bilrost`.
    Error(eyre::Error),
}

impl RoundtripResult {
    /// Unwrap the roundtrip result.
    #[allow(dead_code)]
    pub fn unwrap(self) -> Vec<u8> {
        match self {
            RoundtripResult::Ok(buf) => buf,
            RoundtripResult::DecodeError(error) => {
                panic!("failed to decode the roundtrip data: {}", error)
            }
            RoundtripResult::Error(error) => panic!("failed roundtrip: {}", error),
        }
    }

    /// Unwrap the roundtrip result. Panics if the result was a validation or re-encoding error.
    pub fn unwrap_error(self) -> Result<Vec<u8>, DecodeError> {
        match self {
            RoundtripResult::Ok(buf) => Ok(buf),
            RoundtripResult::DecodeError(error) => Err(error),
            RoundtripResult::Error(error) => panic!("failed roundtrip: {}", error),
        }
    }
}

fn roundtrip<M>(data: &[u8]) -> RoundtripResult
where
    M: OwnedMessage,
{
    // Try to decode a message from the data. If decoding fails, continue.
    let message = match M::decode(data) {
        Ok(decoded) => decoded,
        Err(error) => return RoundtripResult::DecodeError(error),
    };

    let encoded_len = message.encoded_len();

    let buf1 = message.encode_to_vec();
    if encoded_len != buf1.len() {
        return RoundtripResult::Error(err!(
            "expected encoded len ({}) did not match actual encoded len ({})",
            encoded_len,
            buf1.len()
        ));
    }

    let prepend_buf = message.encode_fast();

    if encoded_len != prepend_buf.len() {
        return RoundtripResult::Error(err!(
            "expected encoded len ({}) did not match actual prepended len ({})",
            encoded_len,
            prepend_buf.len()
        ));
    }

    let mut prepended = Vec::new();
    prepended.put(prepend_buf);
    if prepended != buf1 {
        return RoundtripResult::Error(err!("encoded and prepended messages were different"));
    }

    let roundtrip = match M::decode(buf1.as_slice()) {
        Ok(roundtrip) => roundtrip,
        Err(error) => return RoundtripResult::Error(err!(error)),
    };

    let buf2 = roundtrip.encode_to_vec();
    let buf3_rev = roundtrip.encode_fast();
    let mut buf3 = Vec::new();
    buf3.put(buf3_rev);

    /*
    // Useful for debugging:
    eprintln!(" data: {:?}", data.iter().map(|x| format!("0x{:x}", x)).collect::<Vec<_>>());
    eprintln!(" buf1: {:?}", buf1.iter().map(|x| format!("0x{:x}", x)).collect::<Vec<_>>());
    eprintln!("a: {:?}\nb: {:?}", all_types, roundtrip);
    */

    if buf1 != buf2 {
        return RoundtripResult::Error(err!("roundtripped encoded buffers do not match"));
    }

    if buf1 != buf3 {
        return RoundtripResult::Error(err!(
            "roundtripped encoded buffers do not match with prepend-encoding"
        ));
    }

    RoundtripResult::Ok(buf1)
}

fn roundtrip_distinguished<M>(data: &[u8]) -> RoundtripResult
where
    M: DistinguishedOwnedMessage + Eq,
{
    // Try to decode a message from the data. If decoding fails, continue.
    let (message, canon) = match M::decode_distinguished(data) {
        Ok(decoded) => decoded,
        Err(error) => return RoundtripResult::DecodeError(error),
    };

    let encoded_len = message.encoded_len();

    let buf1 = message.encode_to_vec();
    if encoded_len != buf1.len() {
        return RoundtripResult::Error(err!(
            "expected encoded len ({}) did not match actual encoded len ({})",
            encoded_len,
            buf1.len()
        ));
    }

    match canon {
        Canonical => {
            if buf1.as_slice() != data {
                return RoundtripResult::Error(err!("decoded canonically but did not round trip"));
            }
        }
        _ => {
            if M::decode_restricted(data, Canonical).is_ok() {
                return RoundtripResult::Error(err!(
                    "decoded non-canonically but restricted mode did not err"
                ));
            };
            if canon == NotCanonical {
                let Err(err) = M::decode_restricted(data, HasExtensions) else {
                    return RoundtripResult::Error(err!(
                        "decoded non-canonically but partially restricted mode did not err"
                    ));
                };
                if err.kind() != DecodeErrorKind::NotCanonical {
                    return RoundtripResult::Error(err!(
                        "decoded non-canonically but partially restricted mode produced the wrong \
                        error: {err}"
                    ));
                }
            }
            if buf1.as_slice() == data {
                return RoundtripResult::Error(err!(
                    "decoded non-canonically but round tripped unchanged"
                ));
            }
        }
    }

    let prepend_buf = message.encode_fast();

    if encoded_len != prepend_buf.len() {
        return RoundtripResult::Error(err!(
            "expected encoded len ({}) did not match actual prepended len ({})",
            encoded_len,
            prepend_buf.len()
        ));
    }

    let mut prepended = Vec::new();
    prepended.put(prepend_buf);
    if prepended != buf1 {
        return RoundtripResult::Error(err!("encoded and prepended messages were different",));
    }

    let roundtrip = match M::decode_canonical(buf1.as_slice()) {
        Ok(roundtrip) => roundtrip,
        Err(error) => return RoundtripResult::Error(err!(error)),
    };

    if roundtrip != message {
        return RoundtripResult::Error(err!("roundtripped message structs are not equal"));
    }

    let buf2 = roundtrip.encode_to_vec();
    let buf3_rev = roundtrip.encode_fast();
    let mut buf3 = Vec::new();
    buf3.put(buf3_rev);

    /*
    // Useful for debugging:
    eprintln!(" data: {:?}", data.iter().map(|x| format!("0x{:x}", x)).collect::<Vec<_>>());
    eprintln!(" buf1: {:?}", buf1.iter().map(|x| format!("0x{:x}", x)).collect::<Vec<_>>());
    eprintln!("a: {:?}\nb: {:?}", all_types, roundtrip);
    */

    if buf1 != buf2 {
        return RoundtripResult::Error(err!("roundtripped encoded buffers do not match"));
    }

    if buf1 != buf3 {
        return RoundtripResult::Error(err!(
            "roundtripped encoded buffers do not match with prepend-encoding"
        ));
    }

    RoundtripResult::Ok(buf1)
}

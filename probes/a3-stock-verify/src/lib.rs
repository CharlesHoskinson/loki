use ff::PrimeField;
use midnight_curves::Fq;

#[derive(Debug)]
pub enum InstanceError {
    Json(serde_json::Error),
    WrongLaneCount { actual: usize, expected: usize },
    InvalidHex { index: usize, detail: String },
    WrongFieldWidth { index: usize, actual: usize },
    NonCanonicalField { index: usize },
}

impl std::fmt::Display for InstanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid instance JSON: {error}"),
            Self::WrongLaneCount { actual, expected } => {
                write!(f, "instance has {actual} lanes; expected {expected}")
            }
            Self::InvalidHex { index, detail } => {
                write!(f, "instance lane {index} is not valid hex: {detail}")
            }
            Self::WrongFieldWidth { index, actual } => {
                write!(f, "instance lane {index} decodes to {actual} bytes; expected 32")
            }
            Self::NonCanonicalField { index } => {
                write!(f, "instance lane {index} is not a canonical Fq encoding")
            }
        }
    }
}

impl std::error::Error for InstanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

pub fn parse_instance_json(bytes: &[u8], expected: usize) -> Result<Vec<Fq>, InstanceError> {
    let fields: Vec<String> = serde_json::from_slice(bytes).map_err(InstanceError::Json)?;
    if fields.len() != expected {
        return Err(InstanceError::WrongLaneCount {
            actual: fields.len(),
            expected,
        });
    }

    fields
        .into_iter()
        .enumerate()
        .map(|(index, field)| {
            let decoded = hex::decode(&field).map_err(|error| InstanceError::InvalidHex {
                index,
                detail: error.to_string(),
            })?;
            if decoded.len() != 32 {
                return Err(InstanceError::WrongFieldWidth {
                    index,
                    actual: decoded.len(),
                });
            }
            let mut repr = <Fq as PrimeField>::Repr::default();
            repr.as_mut().copy_from_slice(&decoded);
            Option::<Fq>::from(Fq::from_repr(repr))
                .ok_or(InstanceError::NonCanonicalField { index })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ff::{Field, PrimeField};
    use midnight_curves::Fq;
    use serde_json::json;

    use crate::{InstanceError, parse_instance_json};

    const LANES: usize = 28;

    fn field_hex(value: Fq) -> String {
        hex::encode(value.to_repr().as_ref())
    }

    fn valid_instance_json() -> Vec<u8> {
        serde_json::to_vec(&(0..LANES).map(|i| field_hex(Fq::from(i as u64))).collect::<Vec<_>>())
            .unwrap()
    }

    #[test]
    fn accepts_exactly_28_canonical_field_lanes() {
        let parsed = parse_instance_json(&valid_instance_json(), LANES).unwrap();
        assert_eq!(parsed.len(), LANES);
        assert_eq!(parsed[0], Fq::ZERO);
        assert_eq!(parsed[27], Fq::from(27));
    }

    #[test]
    fn rejects_wrong_lane_count() {
        let short = serde_json::to_vec(&vec![field_hex(Fq::ZERO); LANES - 1]).unwrap();
        assert!(matches!(
            parse_instance_json(&short, LANES),
            Err(InstanceError::WrongLaneCount { actual: 27, expected: 28 })
        ));

        let long = serde_json::to_vec(&vec![field_hex(Fq::ZERO); LANES + 1]).unwrap();
        assert!(matches!(
            parse_instance_json(&long, LANES),
            Err(InstanceError::WrongLaneCount { actual: 29, expected: 28 })
        ));
    }

    #[test]
    fn rejects_malformed_json_and_non_string_lanes() {
        assert!(matches!(
            parse_instance_json(b"{", LANES),
            Err(InstanceError::Json(_))
        ));
        let non_string = serde_json::to_vec(&json!(vec![0; 28])).unwrap();
        assert!(matches!(
            parse_instance_json(&non_string, LANES),
            Err(InstanceError::Json(_))
        ));
    }

    #[test]
    fn rejects_bad_hex_width_and_noncanonical_fields() {
        let mut lanes = vec![field_hex(Fq::ZERO); LANES];
        lanes[3] = "zz".to_string();
        let bad_hex = serde_json::to_vec(&lanes).unwrap();
        assert!(matches!(
            parse_instance_json(&bad_hex, LANES),
            Err(InstanceError::InvalidHex { index: 3, .. })
        ));

        lanes[3] = "00".repeat(31);
        let bad_width = serde_json::to_vec(&lanes).unwrap();
        assert!(matches!(
            parse_instance_json(&bad_width, LANES),
            Err(InstanceError::WrongFieldWidth { index: 3, actual: 31 })
        ));

        lanes[3] = "ff".repeat(32);
        let noncanonical = serde_json::to_vec(&lanes).unwrap();
        assert!(matches!(
            parse_instance_json(&noncanonical, LANES),
            Err(InstanceError::NonCanonicalField { index: 3 })
        ));
    }
}

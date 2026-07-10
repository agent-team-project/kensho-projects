use super::DateSystem;

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Convert an Excel date serial into a year-month-day tuple.
///
/// Excel's 1900 date system preserves a historical compatibility quirk:
/// serial 60 is the fictitious date 1900-02-29.
pub fn serial_to_ymd(serial: f64, sys: DateSystem) -> (i32, u32, u32) {
    match sys {
        DateSystem::Excel1900 => {
            let day = serial.floor() as i64;

            if day == 60 {
                return (1900, 2, 29);
            }

            let real_day = if day > 60 { day - 1 } else { day };
            civil_from_days(days_from_civil(1899, 12, 31) + real_day)
        }
    }
}

/// Convert a year-month-day tuple into an Excel date serial.
///
/// Callers are responsible for user-facing validation. The one non-Gregorian
/// date accepted by the Excel1900 system is the phantom 1900-02-29.
pub fn ymd_to_serial(y: i32, m: u32, d: u32, sys: DateSystem) -> f64 {
    match sys {
        DateSystem::Excel1900 => {
            if (y, m, d) == (1900, 2, 29) {
                return 60.0;
            }

            let serial = days_from_civil(y, m, d) - days_from_civil(1899, 12, 31);
            let excel_serial = if serial >= 60 { serial + 1 } else { serial };
            excel_serial as f64
        }
    }
}

/// Convert a time of day into Excel's fractional-day representation.
pub fn time_to_fraction(h: u32, min: u32, s: u32) -> f64 {
    (h as f64 * 3_600.0 + min as f64 * 60.0 + s as f64) / SECONDS_PER_DAY
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };

    if month <= 2 {
        year += 1;
    }

    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn serial_one_round_trips_to_1900_01_01() {
        assert_eq!(ymd_to_serial(1900, 1, 1, SYS), 1.0);
        assert_eq!(serial_to_ymd(1.0, SYS), (1900, 1, 1));
    }

    #[test]
    fn serial_59_is_1900_02_28() {
        assert_eq!(ymd_to_serial(1900, 2, 28, SYS), 59.0);
        assert_eq!(serial_to_ymd(59.0, SYS), (1900, 2, 28));
    }

    #[test]
    fn serial_60_is_the_phantom_1900_02_29() {
        assert_eq!(ymd_to_serial(1900, 2, 29, SYS), 60.0);
        assert_eq!(serial_to_ymd(60.0, SYS), (1900, 2, 29));
    }

    #[test]
    fn serial_61_is_1900_03_01() {
        assert_eq!(ymd_to_serial(1900, 3, 1, SYS), 61.0);
        assert_eq!(serial_to_ymd(61.0, SYS), (1900, 3, 1));
    }

    #[test]
    fn ymd_2020_01_01_is_serial_43831() {
        assert_eq!(ymd_to_serial(2020, 1, 1, SYS), 43_831.0);
        assert_eq!(serial_to_ymd(43_831.0, SYS), (2020, 1, 1));
    }

    #[test]
    fn leap_year_2024_february_has_29_days() {
        let start = ymd_to_serial(2024, 2, 1, SYS);
        let end = ymd_to_serial(2024, 3, 1, SYS);

        assert_eq!(end - start, 29.0);
    }

    #[test]
    fn serial_to_ymd_uses_date_portion_for_positive_fractional_serials() {
        assert_eq!(serial_to_ymd(43_831.75, SYS), (2020, 1, 1));
    }

    #[test]
    fn noon_is_half_a_day() {
        assert_eq!(time_to_fraction(12, 0, 0), 0.5);
    }
}

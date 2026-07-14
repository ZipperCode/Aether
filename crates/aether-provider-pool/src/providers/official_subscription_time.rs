pub(super) fn rfc3339_unix_secs(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<u32>().ok()?;
    let day = value.get(8..10)?.parse::<u32>().ok()?;
    let hour = value.get(11..13)?.parse::<u32>().ok()?;
    let minute = value.get(14..16)?.parse::<u32>().ok()?;
    let second = value.get(17..19)?.parse::<u32>().ok()?;
    if hour > 23 || minute > 59 || second > 59 || day == 0 || day > days_in_month(year, month)? {
        return None;
    }
    let suffix = value.get(19..)?;
    let zone = if let Some(fraction) = suffix.strip_prefix('.') {
        let offset = fraction.find(['Z', '+', '-'])?;
        if offset == 0
            || !fraction
                .get(..offset)?
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        fraction.get(offset..)?
    } else {
        suffix
    };
    let offset_secs = timezone_offset_secs(zone)?;
    let days = days_from_civil(year, month, day)?;
    let local_secs = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour).checked_mul(3_600)?)?
        .checked_add(i64::from(minute).checked_mul(60)?)?
        .checked_add(i64::from(second))?;
    u64::try_from(local_secs.checked_sub(offset_secs)?).ok()
}

fn timezone_offset_secs(zone: &str) -> Option<i64> {
    if zone == "Z" {
        return Some(0);
    }
    let sign = match zone.as_bytes().first()? {
        b'+' => 1i64,
        b'-' => -1i64,
        _ => return None,
    };
    if zone.as_bytes().get(3) != Some(&b':') || zone.len() != 6 {
        return None;
    }
    let hours = zone.get(1..3)?.parse::<u32>().ok()?;
    let minutes = zone.get(4..6)?.parse::<u32>().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (i64::from(hours) * 3_600 + i64::from(minutes) * 60))
}

fn days_in_month(year: i64, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    let adjusted_year = year.checked_sub(i64::from(month <= 2))?;
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year.checked_sub(399)?
    } / 400;
    let year_of_era = adjusted_year.checked_sub(era.checked_mul(400)?)?;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era.checked_mul(146_097)?
        .checked_add(day_of_era)?
        .checked_sub(719_468)
}

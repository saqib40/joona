use std::time::{Duration, Instant};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, EventField, KeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use crate::hotkeys::{JOONA_INPUT_MARKER, modifiers_released};
use crate::insert::{Stroke, strokes};

pub fn wait_for_modifiers(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !modifiers_released() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

pub fn paste() -> anyhow::Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow::anyhow!("failed to create CGEventSource"))?;
    let down = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true)
        .map_err(|_| anyhow::anyhow!("failed to create Cmd+V down event"))?;
    let up = CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, false)
        .map_err(|_| anyhow::anyhow!("failed to create Cmd+V up event"))?;

    down.set_flags(CGEventFlags::CGEventFlagCommand);
    up.set_flags(CGEventFlags::CGEventFlagCommand);

    down.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        JOONA_INPUT_MARKER as i64,
    );
    up.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        JOONA_INPUT_MARKER as i64,
    );

    down.post(CGEventTapLocation::Session);
    up.post(CGEventTapLocation::Session);
    Ok(())
}

pub fn copy() -> anyhow::Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow::anyhow!("failed to create CGEventSource"))?;
    let down = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_C, true)
        .map_err(|_| anyhow::anyhow!("failed to create Cmd+C down event"))?;
    let up = CGEvent::new_keyboard_event(source, KeyCode::ANSI_C, false)
        .map_err(|_| anyhow::anyhow!("failed to create Cmd+C up event"))?;

    down.set_flags(CGEventFlags::CGEventFlagCommand);
    up.set_flags(CGEventFlags::CGEventFlagCommand);

    down.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        JOONA_INPUT_MARKER as i64,
    );
    up.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        JOONA_INPUT_MARKER as i64,
    );

    down.post(CGEventTapLocation::Session);
    up.post(CGEventTapLocation::Session);
    Ok(())
}

pub fn press_enter() -> anyhow::Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow::anyhow!("failed to create CGEventSource"))?;
    let down = CGEvent::new_keyboard_event(source.clone(), KeyCode::RETURN, true)
        .map_err(|_| anyhow::anyhow!("failed to create Return down event"))?;
    let up = CGEvent::new_keyboard_event(source, KeyCode::RETURN, false)
        .map_err(|_| anyhow::anyhow!("failed to create Return up event"))?;

    down.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        JOONA_INPUT_MARKER as i64,
    );
    up.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        JOONA_INPUT_MARKER as i64,
    );

    down.post(CGEventTapLocation::Session);
    up.post(CGEventTapLocation::Session);
    Ok(())
}

pub fn type_text(text: &str) -> anyhow::Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow::anyhow!("failed to create CGEventSource"))?;

    let mut iter = strokes(text).into_iter().peekable();
    while let Some(stroke) = iter.next() {
        match stroke {
            Stroke::Unit(unit) => {
                let mut units = [unit, 0];
                let slice = if (0xD800..=0xDBFF).contains(&unit)
                    && let Some(&Stroke::Unit(low)) = iter.peek()
                    && (0xDC00..=0xDFFF).contains(&low)
                {
                    iter.next();
                    units[1] = low;
                    &units[..2]
                } else {
                    &units[..1]
                };

                let down = CGEvent::new_keyboard_event(source.clone(), 0, true)
                    .map_err(|_| anyhow::anyhow!("failed to create Unicode key down event"))?;
                let up = CGEvent::new_keyboard_event(source.clone(), 0, false)
                    .map_err(|_| anyhow::anyhow!("failed to create Unicode key up event"))?;

                down.set_string_from_utf16_unchecked(slice);
                up.set_string_from_utf16_unchecked(slice);

                down.set_integer_value_field(
                    EventField::EVENT_SOURCE_USER_DATA,
                    JOONA_INPUT_MARKER as i64,
                );
                up.set_integer_value_field(
                    EventField::EVENT_SOURCE_USER_DATA,
                    JOONA_INPUT_MARKER as i64,
                );

                down.post(CGEventTapLocation::Session);
                up.post(CGEventTapLocation::Session);
            }
            Stroke::NewLine => {
                let down = CGEvent::new_keyboard_event(source.clone(), KeyCode::RETURN, true)
                    .map_err(|_| anyhow::anyhow!("failed to create Return down event"))?;
                let up = CGEvent::new_keyboard_event(source.clone(), KeyCode::RETURN, false)
                    .map_err(|_| anyhow::anyhow!("failed to create Return up event"))?;

                down.set_flags(CGEventFlags::CGEventFlagShift);
                up.set_flags(CGEventFlags::CGEventFlagShift);

                down.set_integer_value_field(
                    EventField::EVENT_SOURCE_USER_DATA,
                    JOONA_INPUT_MARKER as i64,
                );
                up.set_integer_value_field(
                    EventField::EVENT_SOURCE_USER_DATA,
                    JOONA_INPUT_MARKER as i64,
                );

                down.post(CGEventTapLocation::Session);
                up.post(CGEventTapLocation::Session);
            }
        }
    }
    Ok(())
}

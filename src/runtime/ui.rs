use iced::{id::Id, widget::{Column, Row}, Element, Theme};

use crate::{AppMessage, SubscriptionRequest};

pub fn build_tree(module_id: u32, surface_id: Id, node: &WasmUiNode) -> Element<AppMessage> {
    match node {
        WasmUiNode::Row { children } => Row::with_children(
            children
                .iter()
                .map(|child| build_tree(module_id, surface_id, child))
                .collect::<Vec<Element<AppMessage>>>(),
        )
        .into(),
        WasmUiNode::Column { children } => Column::with_children(
            children
                .iter()
                .map(|child| build_tree(module_id, surface_id, child))
                .collect::<Vec<Element<AppMessage>>>(),
        )
        .into(),
        WasmUiNode::Text { content, style } => {
            let mut widget = text(content.clone()).size(11);

            widget = widget.style(Box::new(|_: &Theme| *style));

            widget.into()
        }
        WasmUiNode::Button { inner, callback_id } => {
            let mut widget = button(build_tree(module_id, surface_id, inner));

            if *callback_id != 0 {
                widget = widget.on_press_with(move || {
                    AppMessage::Request(SubscriptionRequest::Wasm(WasmRequest::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: None,
                    }))
                });
            }

            widget.into()
        }
        WasmUiNode::Slider {
            number_type,
            range,
            value,
            callback_id,
        } => match number_type {
            wasm::SliderNumberType::I32 => {
                let start = *range.start() as i32;
                let end = *range.end() as i32;
                let range = start..=end;

                slider(range, *value as i32, move |new_value| {
                    AppMessage::Request(SubscriptionRequest::Wasm(WasmRequest::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: Some(WasmCallbackData::Slider(new_value as u64)),
                    }))
                })
                .into()
            }
            wasm::SliderNumberType::F32 => {
                let start = f32::from_bits(*range.start() as u32);
                let end = f32::from_bits(*range.end() as u32);
                let range = start..=end;

                slider(range, f32::from_bits(*value as u32), move |new_value| {
                    AppMessage::Request(SubscriptionRequest::Wasm(WasmRequest::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: Some(WasmCallbackData::Slider(new_value.to_bits() as u64)),
                    }))
                })
                .into()
            }
            wasm::SliderNumberType::F64 => {
                let start = f64::from_bits(*range.start());
                let end = f64::from_bits(*range.end());
                let range = start..=end;

                slider(range, f64::from_bits(*value), move |new_value| {
                    AppMessage::Request(SubscriptionRequest::Wasm(WasmRequest::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: Some(WasmCallbackData::Slider(new_value.to_bits() as u64)),
                    }))
                })
                .into()
            }
        },
        WasmUiNode::Stack { children } => Stack::with_children(
            children
                .iter()
                .map(|child| build_tree(module_id, surface_id, child))
                .collect::<Vec<Element<AppMessage>>>(),
        )
        .into(),
    }
}

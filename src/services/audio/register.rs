
                        let mut module_registers = module_registers.lock().unwrap();
                        if data.is_set(AudioRegisterData::SINKS_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SinksChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::SinksChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::DEFAULT_SINK_CHANGED) {
                            match module_registers.get_mut(&RegisterType::DefaultSinkChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers
                                        .insert(RegisterType::DefaultSinkChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::SOURCES_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SourcesChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::SourcesChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::DEFAULT_SOURCE_CHANGED) {
                            match module_registers.get_mut(&RegisterType::DefaultSourceChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers
                                        .insert(RegisterType::DefaultSourceChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::CARDS_CHANGED) {
                            match module_registers.get_mut(&RegisterType::CardsChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::CardsChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::SINK_PROFILE_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SinkProfileChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::CardsChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::SOURCE_PROFILE_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SourceProfileChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers
                                        .insert(RegisterType::SourceProfileChanged, vec![id]);
                                }
                            };
                        }

/// the type of register
/// should match up with `Event` above
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum RegisterType {
    SinksChanged,
    DefaultSinkChanged,

    SourcesChanged,
    DefaultSourceChanged,

    CardsChanged,

    SinkProfileChanged,
    SourceProfileChanged,
}


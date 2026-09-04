use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    fmt,
    hash::Hash,
};

use super::{SignalFilter, SignalFilterDefinition, SignalFilterError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessedSignal<SignalId> {
    pub signal_id: SignalId,
    pub timestamp: f64,
    pub value: f64,
}

#[derive(Debug)]
pub struct SignalProcessingGraph<SignalId> {
    nodes: HashMap<SignalId, SignalProcessingNode>,
    outputs_by_input: HashMap<SignalId, Vec<SignalId>>,
}

#[derive(Debug)]
struct SignalProcessingNode {
    processor: SignalProcessor,
}

#[derive(Debug)]
enum SignalProcessor {
    Filter(SignalFilter),
}

impl<SignalId> Default for SignalProcessingGraph<SignalId> {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            outputs_by_input: HashMap::new(),
        }
    }
}

impl<SignalId> SignalProcessingGraph<SignalId>
where
    SignalId: Copy + Eq + Hash,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn contains_output(&self, signal_id: SignalId) -> bool {
        self.nodes.contains_key(&signal_id)
    }

    pub fn add_filter(
        &mut self,
        input: SignalId,
        output: SignalId,
        definition: SignalFilterDefinition,
    ) -> Result<(), SignalProcessingGraphDefinitionError<SignalId>> {
        if self.nodes.contains_key(&output) {
            return Err(SignalProcessingGraphDefinitionError::DuplicateOutput { output });
        }

        if self.would_create_cycle(input, output) {
            return Err(SignalProcessingGraphDefinitionError::Cycle { input, output });
        }

        self.nodes.insert(
            output,
            SignalProcessingNode {
                processor: SignalProcessor::Filter(SignalFilter::new(definition)),
            },
        );

        self.outputs_by_input.entry(input).or_default().push(output);

        Ok(())
    }

    pub fn replace_filter(
        &mut self,
        output: SignalId,
        definition: SignalFilterDefinition,
    ) -> Result<(), SignalProcessingGraphUpdateError<SignalId>> {
        let Some(node) = self.nodes.get_mut(&output) else {
            return Err(SignalProcessingGraphUpdateError::UnknownOutput { output });
        };

        node.processor = SignalProcessor::Filter(SignalFilter::new(definition));

        self.reset_from(output);

        Ok(())
    }

    pub fn process(
        &mut self,
        signal_id: SignalId,
        timestamp: f64,
        value: f64,
    ) -> Result<Vec<ProcessedSignal<SignalId>>, SignalProcessingError<SignalId>> {
        let mut pending = VecDeque::new();

        pending.push_back(ProcessedSignal {
            signal_id,
            timestamp,
            value,
        });

        let mut processed = Vec::new();

        while let Some(input) = pending.pop_front() {
            let dependent_outputs = self
                .outputs_by_input
                .get(&input.signal_id)
                .cloned()
                .unwrap_or_default();

            for output_id in dependent_outputs {
                let node = self
                    .nodes
                    .get_mut(&output_id)
                    .expect("registered processing output must have a node");

                let output_value = node
                    .process(input.timestamp, input.value)
                    .map_err(|error| SignalProcessingError {
                        output: output_id,
                        error,
                    })?;

                let output = ProcessedSignal {
                    signal_id: output_id,
                    timestamp: input.timestamp,
                    value: output_value,
                };

                processed.push(output);
                pending.push_back(output);
            }
        }

        Ok(processed)
    }

    pub fn reset_from(&mut self, signal_id: SignalId) {
        let mut pending = VecDeque::new();
        let mut reset_outputs = HashSet::new();

        pending.push_back(signal_id);

        while let Some(input_id) = pending.pop_front() {
            let dependent_outputs = self
                .outputs_by_input
                .get(&input_id)
                .cloned()
                .unwrap_or_default();

            for output_id in dependent_outputs {
                if !reset_outputs.insert(output_id) {
                    continue;
                }

                let node = self
                    .nodes
                    .get_mut(&output_id)
                    .expect("registered processing output must have a node");

                node.reset();

                pending.push_back(output_id);
            }
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.outputs_by_input.clear();
    }

    pub fn removal_set_from(&self, signal_id: SignalId) -> Vec<SignalId> {
        let mut pending = VecDeque::from([signal_id]);

        let mut affected = Vec::new();
        let mut visited = HashSet::new();

        if self.nodes.contains_key(&signal_id) {
            visited.insert(signal_id);
            affected.push(signal_id);
        }

        while let Some(input) = pending.pop_front() {
            let Some(outputs) = self.outputs_by_input.get(&input) else {
                continue;
            };

            for &output in outputs {
                if visited.insert(output) {
                    affected.push(output);
                    pending.push_back(output);
                }
            }
        }

        affected
    }

    pub fn remove_from(&mut self, signal_id: SignalId) -> Vec<SignalId> {
        let removed = self.removal_set_from(signal_id);

        let removed_set = removed.iter().copied().collect::<HashSet<_>>();

        for output in &removed {
            self.nodes.remove(output);
            self.outputs_by_input.remove(output);
        }

        for outputs in self.outputs_by_input.values_mut() {
            outputs.retain(|output| !removed_set.contains(output));
        }

        self.outputs_by_input
            .retain(|_, outputs| !outputs.is_empty());

        removed
    }

    fn would_create_cycle(&self, input: SignalId, output: SignalId) -> bool {
        if input == output {
            return true;
        }

        let mut pending = VecDeque::new();
        let mut visited = HashSet::new();

        pending.push_back(output);

        while let Some(current) = pending.pop_front() {
            if current == input {
                return true;
            }

            if !visited.insert(current) {
                continue;
            }

            if let Some(dependent_outputs) = self.outputs_by_input.get(&current) {
                pending.extend(dependent_outputs.iter().copied());
            }
        }

        false
    }
}

impl SignalProcessingNode {
    fn process(&mut self, timestamp: f64, value: f64) -> Result<f64, SignalFilterError> {
        match &mut self.processor {
            SignalProcessor::Filter(filter) => filter.process(timestamp, value),
        }
    }

    fn reset(&mut self) {
        match &mut self.processor {
            SignalProcessor::Filter(filter) => {
                filter.reset();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalProcessingGraphDefinitionError<SignalId> {
    DuplicateOutput { output: SignalId },

    Cycle { input: SignalId, output: SignalId },
}

impl<SignalId> fmt::Display for SignalProcessingGraphDefinitionError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateOutput { output } => write!(
                formatter,
                "Signal processing output {output} is already registered",
            ),

            Self::Cycle { input, output } => write!(
                formatter,
                "Adding processing edge {input} -> {output} \
                 would create a cycle",
            ),
        }
    }
}

impl<SignalId> Error for SignalProcessingGraphDefinitionError<SignalId> where
    SignalId: fmt::Debug + fmt::Display
{
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalProcessingGraphUpdateError<SignalId> {
    UnknownOutput { output: SignalId },
}

impl<SignalId> fmt::Display for SignalProcessingGraphUpdateError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOutput { output } => {
                write!(
                    formatter,
                    "Signal processing output {output} \
                     is not registered",
                )
            }
        }
    }
}

impl<SignalId> Error for SignalProcessingGraphUpdateError<SignalId> where
    SignalId: fmt::Debug + fmt::Display
{
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignalProcessingError<SignalId> {
    output: SignalId,
    error: SignalFilterError,
}

impl<SignalId> SignalProcessingError<SignalId>
where
    SignalId: Copy,
{
    pub const fn output(&self) -> SignalId {
        self.output
    }

    pub const fn filter_error(&self) -> SignalFilterError {
        self.error
    }
}

impl<SignalId> fmt::Display for SignalProcessingError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Failed to calculate processed signal {}: {}",
            self.output, self.error,
        )
    }
}

impl<SignalId> Error for SignalProcessingError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProcessedSignal, SignalProcessingError, SignalProcessingGraph,
        SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError,
    };

    use crate::signal_processing::{SignalFilterDefinition, SignalFilterError};

    #[test]
    fn processes_signal_through_filter() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        assert_eq!(
            graph.process(1, 0.0, 10.0).unwrap(),
            vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 10.0,
            }],
        );

        assert_eq!(
            graph.process(1, 1.0, 20.0).unwrap(),
            vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 1.0,
                value: 15.0,
            }],
        );
    }

    #[test]
    fn processes_multiple_filters_from_one_input() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph
            .add_filter(1_u64, 3_u64, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        graph.process(1, 0.0, 10.0).unwrap();

        assert_eq!(
            graph.process(1, 1.0, 2.0).unwrap(),
            vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 1.0,
                    value: 6.0,
                },
                ProcessedSignal {
                    signal_id: 3,
                    timestamp: 1.0,
                    value: 6.0,
                },
            ],
        );
    }

    #[test]
    fn processes_filter_chain() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph
            .add_filter(2_u64, 3_u64, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        assert_eq!(
            graph.process(1, 0.0, 10.0).unwrap(),
            vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 0.0,
                    value: 10.0,
                },
                ProcessedSignal {
                    signal_id: 3,
                    timestamp: 0.0,
                    value: 10.0,
                },
            ],
        );

        assert_eq!(
            graph.process(1, 1.0, 20.0).unwrap(),
            vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 1.0,
                    value: 15.0,
                },
                ProcessedSignal {
                    signal_id: 3,
                    timestamp: 1.0,
                    value: 12.5,
                },
            ],
        );
    }

    #[test]
    fn rejects_duplicate_output() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        assert_eq!(
            graph.add_filter(3, 2, SignalFilterDefinition::median(3).unwrap(),),
            Err(SignalProcessingGraphDefinitionError::DuplicateOutput { output: 2 },),
        );
    }

    #[test]
    fn rejects_direct_cycle() {
        let mut graph = SignalProcessingGraph::new();

        assert_eq!(
            graph.add_filter(1_u64, 1_u64, SignalFilterDefinition::median(3).unwrap(),),
            Err(SignalProcessingGraphDefinitionError::Cycle {
                input: 1,
                output: 1,
            }),
        );
    }

    #[test]
    fn rejects_indirect_cycle() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph
            .add_filter(2_u64, 3_u64, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        assert_eq!(
            graph.add_filter(3, 1, SignalFilterDefinition::exponential(1.0).unwrap(),),
            Err(SignalProcessingGraphDefinitionError::Cycle {
                input: 3,
                output: 1,
            }),
        );
    }

    #[test]
    fn resets_all_dependent_filters() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph
            .add_filter(
                2_u64,
                3_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph.process(1, 10.0, 10.0).unwrap();
        graph.process(1, 11.0, 20.0).unwrap();

        graph.reset_from(1);

        assert_eq!(
            graph.process(1, 0.0, 100.0).unwrap(),
            vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 0.0,
                    value: 100.0,
                },
                ProcessedSignal {
                    signal_id: 3,
                    timestamp: 0.0,
                    value: 100.0,
                },
            ],
        );
    }

    #[test]
    fn reports_filter_processing_error() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph.process(1, 1.0, 10.0).unwrap();

        let error = graph.process(1, 1.0, 20.0).unwrap_err();

        assert_eq!(
            error,
            SignalProcessingError {
                output: 2,
                error: SignalFilterError::NonIncreasingTimestamp {
                    previous: 1.0,
                    current: 1.0,
                },
            },
        );

        assert_eq!(error.output(), 2);

        assert_eq!(
            error.filter_error(),
            SignalFilterError::NonIncreasingTimestamp {
                previous: 1.0,
                current: 1.0,
            },
        );
    }

    #[test]
    fn clears_graph() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        assert_eq!(graph.len(), 1);
        assert!(graph.contains_output(2));

        graph.clear();

        assert!(graph.is_empty());
        assert!(!graph.contains_output(2));
        assert!(graph.process(1, 0.0, 10.0).unwrap().is_empty());
    }

    #[test]
    fn removes_filter_and_all_dependent_filters() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(1_u64, 2, SignalFilterDefinition::moving_average(3).unwrap())
            .unwrap();

        graph
            .add_filter(2, 3, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        graph
            .add_filter(1, 4, SignalFilterDefinition::exponential(1.0).unwrap())
            .unwrap();

        assert_eq!(graph.remove_from(2), vec![2, 3]);

        assert!(graph.remove_from(4).contains(&4));

        assert!(graph.remove_from(2).is_empty());
    }

    #[test]
    fn removes_all_filters_derived_from_raw_signal() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(1_u64, 2, SignalFilterDefinition::moving_average(3).unwrap())
            .unwrap();

        graph
            .add_filter(1, 3, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        graph
            .add_filter(2, 4, SignalFilterDefinition::exponential(1.0).unwrap())
            .unwrap();

        assert_eq!(graph.remove_from(1), vec![2, 3, 4]);
        assert!(graph.remove_from(1).is_empty());
    }

    #[test]
    fn replaces_filter_and_resets_its_state() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph.process(1, 0.0, 10.0).unwrap();

        assert_eq!(
            graph.process(1, 1.0, 20.0).unwrap(),
            vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 1.0,
                value: 15.0,
            }],
        );

        graph
            .replace_filter(2, SignalFilterDefinition::moving_average(3).unwrap())
            .unwrap();

        assert_eq!(
            graph.process(1, 2.0, 100.0).unwrap(),
            vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 2.0,
                value: 100.0,
            }],
        );
    }

    #[test]
    fn replacing_filter_resets_dependent_filters() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(
                1_u64,
                2_u64,
                SignalFilterDefinition::moving_average(2).unwrap(),
            )
            .unwrap();

        graph
            .add_filter(2, 3, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        graph.process(1, 0.0, 10.0).unwrap();
        graph.process(1, 1.0, 20.0).unwrap();

        graph
            .replace_filter(2, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        assert_eq!(
            graph.process(1, 2.0, 100.0).unwrap(),
            vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 2.0,
                    value: 100.0,
                },
                ProcessedSignal {
                    signal_id: 3,
                    timestamp: 2.0,
                    value: 100.0,
                },
            ],
        );
    }

    #[test]
    fn rejects_replacing_unknown_filter() {
        let mut graph = SignalProcessingGraph::<u64>::new();

        assert_eq!(
            graph.replace_filter(10, SignalFilterDefinition::median(3).unwrap(),),
            Err(SignalProcessingGraphUpdateError::UnknownOutput { output: 10 },),
        );
    }

    #[test]
    fn removal_preview_matches_actual_removal() {
        let mut graph = SignalProcessingGraph::new();

        graph
            .add_filter(1_u64, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        graph
            .add_filter(2, 3, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        graph
            .add_filter(1, 4, SignalFilterDefinition::exponential(1.0).unwrap())
            .unwrap();

        let preview = graph.removal_set_from(1);

        let removed = graph.remove_from(1);

        assert_eq!(preview, removed,);
    }
}

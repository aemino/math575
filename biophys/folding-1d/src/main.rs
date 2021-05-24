use std::{env, fmt::Debug, iter};

use rayon::prelude::*;

trait Nucleotide {
    fn can_pair(a: &Self, b: &Self) -> bool;

    fn pairs_with(&self, other: &Self) -> bool {
        Self::can_pair(self, other)
    }
}

#[derive(Debug, Clone, Copy)]
enum RnaNucleotide {
    A,
    C,
    G,
    U,
}

impl Nucleotide for RnaNucleotide {
    fn can_pair(a: &Self, b: &Self) -> bool {
        use RnaNucleotide::*;

        match (a, b) {
            (A, U) | (U, A) => true,
            (C, G) | (G, C) => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
enum RnaSegment {
    Single(RnaNucleotide),
    Loop(Vec<RnaNucleotide>),
}

impl RnaSegment {
    fn as_loop(&self) -> Self {
        match self {
            RnaSegment::Single(base) => RnaSegment::Loop(vec![base.clone()]),
            RnaSegment::Loop(_) => self.clone(),
        }
    }
}

impl Debug for RnaSegment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RnaSegment::Single(base) => base.fmt(formatter)?,
            RnaSegment::Loop(bases) => {
                formatter.write_str("{")?;
                bases.iter().try_for_each(|base| base.fmt(formatter))?;
                formatter.write_str("}")?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Default)]
struct RnaStructure {
    segments: Vec<RnaSegment>,
}

impl Debug for RnaStructure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter
            .debug_struct("RnaStructure")
            .field("segments", &{
                let mut buf = String::new();

                for segment in &self.segments {
                    buf.push_str(&format!("{:?}", segment));
                }

                buf
            })
            .finish()
    }
}

impl RnaStructure {
    fn split_at_major_loop(&self) -> (Self, RnaSegment, Self) {
        let (major_loop_idx, major_loop) = self
            .segments
            .iter()
            .enumerate()
            .max_by_key(|(_, segment)| match segment {
                RnaSegment::Loop(bases) => bases.len(),
                _ => 0,
            })
            .expect("expected major loop");

        (
            RnaStructure {
                segments: self.segments[..major_loop_idx]
                    .iter()
                    .rev()
                    .cloned()
                    .collect(),
                ..self.clone()
            },
            major_loop.clone(),
            RnaStructure {
                segments: self.segments[major_loop_idx + 1..]
                    .iter()
                    .cloned()
                    .collect(),
                ..self.clone()
            },
        )
    }

    fn with_first_single_looped(&self) -> Self {
        let single_idx = self
            .segments
            .iter()
            .position(|segment| match segment {
                RnaSegment::Single(_) => true,
                _ => false,
            })
            .unwrap();

        RnaStructure {
            segments: {
                let mut segments = self.segments.clone();
                segments[single_idx] = segments[single_idx].as_loop();

                segments
            },
            ..self.clone()
        }
    }

    fn split_after_first_segment(&self) -> (Self, Self) {
        (
            RnaStructure {
                segments: self.segments[..1].to_vec(),
                ..self.clone()
            },
            RnaStructure {
                segments: self.segments[1..].to_vec(),
                ..self.clone()
            },
        )
    }

    fn split_at_first_pair(&self, other: &Self) -> (Self, Self, Self, Self) {
        match (self.segments.first(), other.segments.first()) {
            (Some(RnaSegment::Loop(_)), Some(RnaSegment::Single(_))) => (
                RnaStructure {
                    segments: self.segments[..1].to_vec(),
                    ..self.clone()
                },
                RnaStructure {
                    segments: self.segments[1..].to_vec(),
                    ..self.clone()
                },
                RnaStructure {
                    segments: vec![],
                    ..other.clone()
                },
                other.clone(),
            ),
            (Some(RnaSegment::Single(_)), Some(RnaSegment::Loop(_))) => (
                RnaStructure {
                    segments: vec![],
                    ..self.clone()
                },
                self.clone(),
                RnaStructure {
                    segments: other.segments[..1].to_vec(),
                    ..other.clone()
                },
                RnaStructure {
                    segments: other.segments[1..].to_vec(),
                    ..other.clone()
                },
            ),
            _ => (
                RnaStructure {
                    segments: self.segments[..1].to_vec(),
                    ..self.clone()
                },
                RnaStructure {
                    segments: self.segments[1..].to_vec(),
                    ..self.clone()
                },
                RnaStructure {
                    segments: other.segments[..1].to_vec(),
                    ..other.clone()
                },
                RnaStructure {
                    segments: other.segments[1..].to_vec(),
                    ..other.clone()
                },
            ),
        }
    }

    fn join(&mut self, mut other: Self) {
        if let Some(RnaSegment::Loop(tail_loop)) = self.segments.last_mut() {
            if let Some(RnaSegment::Loop(head_loop)) = other.segments.first_mut() {
                tail_loop.append(head_loop);
                other.segments.remove(0);
            }
        }

        self.segments.append(&mut other.segments);
    }

    fn paired_free_energy(&self, other: &Self) -> usize {
        let mut free_energy_a = 0;
        let mut free_energy_b = 0;

        let strand_a = self
            .segments
            .iter()
            .filter_map(|segment| match segment {
                RnaSegment::Loop(bases) => {
                    free_energy_a += bases.len();
                    None
                }
                RnaSegment::Single(base) => {
                    free_energy_a += 1;
                    Some(base)
                }
            })
            .collect::<Vec<_>>();

        let strand_b = other
            .segments
            .iter()
            .filter_map(|segment| match segment {
                RnaSegment::Loop(bases) => {
                    free_energy_b += bases.len();
                    None
                }
                RnaSegment::Single(base) => {
                    free_energy_b += 1;
                    Some(base)
                }
            })
            .collect::<Vec<_>>();

        let h_bonds = strand_a
            .into_iter()
            .zip(strand_b.into_iter())
            .filter(|pair| Nucleotide::can_pair(pair.0, pair.1))
            .count();

        free_energy_a + free_energy_b - (h_bonds * 2)
    }

    fn strand_permute_search(strand_a: &Self, strand_b: &Self) -> ((Self, Self), usize) {
        [
            (strand_a, strand_b),
            (&strand_a.with_first_single_looped(), strand_b),
            (strand_a, &strand_b.with_first_single_looped()),
        ]
        .par_iter()
        .map(|&(strand_a, strand_b)| {
            let (mut a_head, a_tail, mut b_head, b_tail) = strand_a.split_at_first_pair(&strand_b);

            let mut free_energy = a_head.paired_free_energy(&b_head);
            // println!("{:?} | {:?} => {}", a_head, b_head, free_energy);

            if !a_tail.segments.is_empty() && !b_tail.segments.is_empty() {
                let ((opt_a_tail, opt_b_tail), opt_free_energy) =
                    Self::strand_permute_search(&a_tail, &b_tail);

                a_head.join(opt_a_tail);
                b_head.join(opt_b_tail);

                free_energy += opt_free_energy;
            } else {
                free_energy += a_tail.paired_free_energy(&b_tail);

                a_head.join(a_tail);
                b_head.join(b_tail);
            }

            ((a_head, b_head), free_energy)
        })
        .min_by_key(|(_, free_energy)| *free_energy)
        .unwrap()
    }

    fn minimize_free_energy(&self) -> (Self, usize, usize) {
        let (strand_a, loop_segment, strand_b) = self.split_at_major_loop();
        let initial_free_energy = strand_a.paired_free_energy(&strand_b);

        let ((opt_strand_a, opt_strand_b), opt_free_energy) =
            Self::strand_permute_search(&strand_a, &strand_b);

        let loop_free_energy = match &loop_segment {
            RnaSegment::Loop(bases) => bases.len(),
            _ => unreachable!(),
        };

        let opt_self = Self {
            segments: opt_strand_a
                .segments
                .into_iter()
                .rev()
                .chain(iter::once(loop_segment))
                .chain(opt_strand_b.segments)
                .collect(),
        };

        (
            opt_self,
            initial_free_energy + loop_free_energy,
            opt_free_energy + loop_free_energy,
        )
    }
}

fn parse_single(token: char) -> Result<RnaNucleotide, String> {
    match token {
        'a' | 'A' => Ok(RnaNucleotide::A),
        'c' | 'C' => Ok(RnaNucleotide::C),
        'g' | 'G' => Ok(RnaNucleotide::G),
        'u' | 'U' => Ok(RnaNucleotide::U),
        _ => Err(format!("unexpected nucleotide token {}", token)),
    }
}

fn parse_nucleotides(sequence: &str) -> Result<Vec<RnaNucleotide>, String> {
    sequence
        .chars()
        .map(parse_single)
        .take_while(Result::is_ok)
        .collect()
}

fn parse_sequence(mut sequence: &str) -> Result<RnaStructure, String> {
    let mut segments = Vec::new();

    while let Some(token) = sequence.chars().next() {
        let segment = match token {
            '{' => {
                let (head, tail) = sequence[1..]
                    .split_once('}')
                    .ok_or("expected loop end token '}'")?;

                sequence = tail;

                RnaSegment::Loop(parse_nucleotides(head)?)
            }
            _ => {
                sequence = &sequence[1..];

                RnaSegment::Single(parse_single(token)?)
            }
        };

        segments.push(segment);
    }

    Ok(RnaStructure { segments })
}

fn main() {
    let mut args = env::args().skip(1);

    let raw_sequence = args.next().expect("expected RNA sequence");

    let structure = parse_sequence(&raw_sequence).expect("failed to parse RNA sequence");

    let (opt_structure, initial_free_energy, opt_free_energy) = structure.minimize_free_energy();

    println!();
    println!("input -> {:?} (H = {})", structure, initial_free_energy);
    println!("optimized <- {:?} (H = {})", opt_structure, opt_free_energy);
    println!();
}

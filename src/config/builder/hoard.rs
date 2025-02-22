//! This module contains definitions useful for working directly with [`Hoard`]s.
//!
//! A [`Hoard`] is a collection of at least one [`Pile`], where a [`Pile`] is a single file
//! or directory that may appear in different locations on a system depending on that system's
//! configuration. The path used is determined by the most specific match in the *environment
//! condition*, which is a string like `foo|bar|baz` where `foo`, `bar`, and `baz` are the
//! names of [`Environment`](super::environment::Environment)s defined in the configuration file.
//! All environments in the condition must match the current system for its matching path to be
//! used.

use crate::config::builder::envtrie::{EnvTrie, Error as TrieError};
use crate::env_vars::{expand_env_in_path, Error as EnvError};
use crate::hoard::PileConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

type ConfigMultiple = crate::config::hoard::MultipleEntries;
type ConfigSingle = crate::config::hoard::Pile;
type ConfigHoard = crate::config::hoard::Hoard;

/// Errors that may occur while processing a [`Builder`](super::Builder) [`Hoard`] into a [`Config`]
/// [`Hoard`](crate::config::hoard::Hoard).
#[derive(Debug, Error)]
pub enum Error {
    /// Error while evaluating a [`Pile`]'s [`EnvTrie`].
    #[error("error while processing environment requirements: {0}")]
    EnvTrie(#[from] TrieError),
    /// Error while expanding environment variables in a path.
    #[error("error while expanding environment variables in path: {0}")]
    ExpandEnv(#[from] EnvError),
}

/// A single pile in the hoard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pile {
    config: Option<PileConfig>,
    #[serde(flatten)]
    items: HashMap<String, String>,
}

impl Pile {
    fn process_with(
        self,
        envs: &HashMap<String, bool>,
        exclusivity: &[Vec<String>],
    ) -> Result<ConfigSingle, Error> {
        let _span = tracing::debug_span!(
            "process_pile",
            pile = ?self
        )
        .entered();

        let Pile { config, items } = self;
        let trie = EnvTrie::new(&items, exclusivity)?;
        let path = trie.get_path(envs)?.map(expand_env_in_path).transpose()?;

        Ok(ConfigSingle { config, path })
    }

    pub(crate) fn layer_config(&mut self, config: Option<&PileConfig>) {
        PileConfig::layer_options(&mut self.config, config);
    }
}

/// A set of multiple related piles (i.e. in a single hoard).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MultipleEntries {
    config: Option<PileConfig>,
    #[serde(flatten)]
    items: HashMap<String, Pile>,
}

impl MultipleEntries {
    fn process_with(
        self,
        envs: &HashMap<String, bool>,
        exclusivity: &[Vec<String>],
    ) -> Result<ConfigMultiple, Error> {
        let MultipleEntries { config, items } = self;
        let items = items
            .into_iter()
            .map(|(pile, mut entry)| {
                tracing::debug!(%pile, "processing pile");
                entry.layer_config(config.as_ref());
                let _span = tracing::debug_span!("processing_span_outer", name=%pile).entered();
                let entry = entry.process_with(envs, exclusivity)?;
                Ok((pile, entry))
            })
            .collect::<Result<_, Error>>()?;

        Ok(ConfigMultiple { piles: items })
    }

    pub(crate) fn layer_config(&mut self, config: Option<&PileConfig>) {
        PileConfig::layer_options(&mut self.config, config);
    }
}

/// A definition of a Hoard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Hoard {
    /// A single anonymous [`Pile`].
    Single(Pile),
    /// Multiple named [`Pile`]s.
    Multiple(MultipleEntries),
}

impl Hoard {
    /// Resolve with path(s) to use for the `Hoard`.
    ///
    /// Uses the provided information to determine which environment combination is the best match
    /// for each [`Pile`] and thus which path to use for each one.
    ///
    /// # Errors
    ///
    /// Any [`enum@Error`] that occurs while evaluating the `Hoard`.
    pub fn process_with(
        self,
        envs: &HashMap<String, bool>,
        exclusivity: &[Vec<String>],
    ) -> Result<crate::config::hoard::Hoard, Error> {
        match self {
            Hoard::Single(single) => {
                tracing::debug!("processing anonymous pile");
                Ok(ConfigHoard::Anonymous(
                    single.process_with(envs, exclusivity)?,
                ))
            }
            Hoard::Multiple(multiple) => {
                tracing::debug!("processing named pile(s)");
                Ok(ConfigHoard::Named(
                    multiple.process_with(envs, exclusivity)?,
                ))
            }
        }
    }

    pub(crate) fn layer_config(&mut self, config: Option<&PileConfig>) {
        match self {
            Hoard::Single(pile) => pile.layer_config(config),
            Hoard::Multiple(multi) => multi.layer_config(config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hoard::pile_config::{
        AsymmetricEncryption, Config as PileConfig, Encryption, SymmetricEncryption,
    };

    mod config {
        use super::*;

        #[test]
        fn test_layer_configs_both_none() {
            let mut specific = None;
            let general = None;
            PileConfig::layer_options(&mut specific, general);
            assert!(specific.is_none());
        }

        #[test]
        fn test_layer_specific_some_general_none() {
            let mut specific = Some(PileConfig {
                encryption: Some(Encryption::Symmetric(SymmetricEncryption::Password(
                    "password".into(),
                ))),
                ignore: vec![glob::Pattern::new("ignore me").unwrap()],
            });
            let old_specific = specific.clone();
            let general = None;
            PileConfig::layer_options(&mut specific, general);
            assert_eq!(specific, old_specific);
        }

        #[test]
        fn test_layer_specific_none_general_some() {
            let mut specific = None;
            let general = Some(PileConfig {
                encryption: Some(Encryption::Symmetric(SymmetricEncryption::Password(
                    "password".into(),
                ))),
                ignore: vec![glob::Pattern::new("ignore me").unwrap()],
            });
            PileConfig::layer_options(&mut specific, general.as_ref());
            assert_eq!(specific, general);
        }

        #[test]
        fn test_layer_configs_both_some() {
            let mut specific = Some(PileConfig {
                encryption: Some(Encryption::Symmetric(SymmetricEncryption::Password(
                    "password".into(),
                ))),
                ignore: vec![
                    glob::Pattern::new("ignore me").unwrap(),
                    glob::Pattern::new("duplicate").unwrap(),
                ],
            });
            let old_specific = specific.clone();
            let general = Some(PileConfig {
                encryption: Some(Encryption::Asymmetric(AsymmetricEncryption {
                    public_key: "somekey".into(),
                })),
                ignore: vec![
                    glob::Pattern::new("me too").unwrap(),
                    glob::Pattern::new("duplicate").unwrap(),
                ],
            });
            PileConfig::layer_options(&mut specific, general.as_ref());
            assert!(specific.is_some());
            assert_eq!(
                specific.as_ref().unwrap().encryption,
                old_specific.unwrap().encryption
            );
            assert_eq!(
                specific.unwrap().ignore,
                vec![
                    glob::Pattern::new("duplicate").unwrap(),
                    glob::Pattern::new("ignore me").unwrap(),
                    glob::Pattern::new("me too").unwrap(),
                ]
            );
        }
    }

    mod process {
        use super::*;
        use crate::hoard::Pile as RealPile;
        use maplit::hashmap;
        use std::path::PathBuf;

        #[test]
        #[serial_test::serial]
        fn env_vars_are_expanded() {
            let pile = Pile {
                config: None,
                items: hashmap! {
                    "foo".into() => "${HOME}/something".into()
                },
            };

            let home = std::env::var("HOME").expect("failed to read $HOME");
            let expected = RealPile {
                config: None,
                path: Some(PathBuf::from(format!("{}/something", home))),
            };

            let envs = hashmap! { "foo".into() =>  true };
            let result = pile
                .process_with(&envs, &[])
                .expect("pile should process without issues");

            assert_eq!(result, expected);
        }
    }

    mod serde {
        use super::*;
        use maplit::hashmap;
        use serde_test::{assert_de_tokens_error, assert_tokens, Token};

        #[test]
        fn single_entry_no_config() {
            let hoard = Hoard::Single(Pile {
                config: None,
                items: hashmap! {
                    "bar_env|foo_env".to_string() => "/some/path".to_string()
                },
            });

            assert_tokens(
                &hoard,
                &[
                    Token::Map { len: None },
                    Token::Str("config"),
                    Token::None,
                    Token::Str("bar_env|foo_env"),
                    Token::Str("/some/path"),
                    Token::MapEnd,
                ],
            );
        }

        #[test]
        fn single_entry_with_config() {
            let hoard = Hoard::Single(Pile {
                config: Some(PileConfig {
                    encryption: Some(Encryption::Asymmetric(AsymmetricEncryption {
                        public_key: "public key".to_string(),
                    })),
                    ignore: Vec::new(),
                }),
                items: hashmap! {
                    "bar_env|foo_env".to_string() => "/some/path".to_string()
                },
            });

            assert_tokens(
                &hoard,
                &[
                    Token::Map { len: None },
                    Token::Str("config"),
                    Token::Some,
                    Token::Struct {
                        name: "Config",
                        len: 2,
                    },
                    Token::Str("encrypt"),
                    Token::Some,
                    Token::Struct {
                        name: "AsymmetricEncryption",
                        len: 2,
                    },
                    Token::Str("type"),
                    Token::Str("asymmetric"),
                    Token::Str("public_key"),
                    Token::Str("public key"),
                    Token::StructEnd,
                    Token::Str("ignore"),
                    Token::Seq { len: Some(0) },
                    Token::SeqEnd,
                    Token::StructEnd,
                    Token::Str("bar_env|foo_env"),
                    Token::Str("/some/path"),
                    Token::MapEnd,
                ],
            );
        }

        #[test]
        fn multiple_entry_no_config() {
            let hoard = Hoard::Multiple(MultipleEntries {
                config: None,
                items: hashmap! {
                    "item1".to_string() => Pile {
                        config: None,
                        items: hashmap! {
                            "bar_env|foo_env".to_string() => "/some/path".to_string()
                        }
                    },
                },
            });

            assert_tokens(
                &hoard,
                &[
                    Token::Map { len: None },
                    Token::Str("config"),
                    Token::None,
                    Token::Str("item1"),
                    Token::Map { len: None },
                    Token::Str("config"),
                    Token::None,
                    Token::Str("bar_env|foo_env"),
                    Token::Str("/some/path"),
                    Token::MapEnd,
                    Token::MapEnd,
                ],
            );
        }

        #[test]
        fn multiple_entry_with_config() {
            let hoard = Hoard::Multiple(MultipleEntries {
                config: Some(PileConfig {
                    encryption: Some(Encryption::Symmetric(SymmetricEncryption::Password(
                        "correcthorsebatterystaple".into(),
                    ))),
                    ignore: Vec::new(),
                }),
                items: hashmap! {
                    "item1".to_string() => Pile {
                        config: None,
                        items: hashmap! {
                            "bar_env|foo_env".to_string() => "/some/path".to_string()
                        }
                    },
                },
            });

            assert_tokens(
                &hoard,
                &[
                    Token::Map { len: None },
                    Token::Str("config"),
                    Token::Some,
                    Token::Struct {
                        name: "Config",
                        len: 2,
                    },
                    Token::Str("encrypt"),
                    Token::Some,
                    Token::Map { len: Some(2) },
                    Token::Str("type"),
                    Token::Str("symmetric"),
                    Token::Str("password"),
                    Token::Str("correcthorsebatterystaple"),
                    Token::MapEnd,
                    Token::Str("ignore"),
                    Token::Seq { len: Some(0) },
                    Token::SeqEnd,
                    Token::StructEnd,
                    Token::Str("item1"),
                    Token::Map { len: None },
                    Token::Str("config"),
                    Token::None,
                    Token::Str("bar_env|foo_env"),
                    Token::Str("/some/path"),
                    Token::MapEnd,
                    Token::MapEnd,
                ],
            );
        }

        #[test]
        fn test_invalid_glob() {
            assert_de_tokens_error::<PileConfig>(
                &[
                    Token::Struct {
                        name: "Config",
                        len: 2,
                    },
                    Token::Str("encrypt"),
                    Token::None,
                    Token::Str("ignore"),
                    Token::Seq { len: Some(2) },
                    Token::Str("**/valid*"),
                    Token::Str("invalid**"),
                    Token::SeqEnd,
                    Token::StructEnd,
                ],
                "Pattern syntax error near position 6: recursive wildcards must form a single path component",
            );
        }

        #[test]
        fn test_valid_globs() {
            let config = PileConfig {
                encryption: None,
                ignore: vec![
                    glob::Pattern::new("**/valid*").unwrap(),
                    glob::Pattern::new("*/also_valid/**").unwrap(),
                ],
            };

            assert_tokens::<PileConfig>(
                &config,
                &[
                    Token::Struct {
                        name: "Config",
                        len: 2,
                    },
                    Token::Str("encrypt"),
                    Token::None,
                    Token::Str("ignore"),
                    Token::Seq { len: Some(2) },
                    Token::Str("**/valid*"),
                    Token::Str("*/also_valid/**"),
                    Token::SeqEnd,
                    Token::StructEnd,
                ],
            );
        }
    }
}

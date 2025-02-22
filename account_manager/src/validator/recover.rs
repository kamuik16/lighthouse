use super::create::STORE_WITHDRAW_FLAG;
use crate::validator::create::COUNT_FLAG;
use crate::SECRETS_DIR_FLAG;
use account_utils::eth2_keystore::{keypair_from_secret, Keystore, KeystoreBuilder};
use account_utils::{random_password, read_mnemonic_from_cli, STDIN_INPUTS_FLAG};
use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_utils::FLAG_HEADER;
use directory::ensure_dir_exists;
use directory::{parse_path_or_default_with_flag, DEFAULT_SECRET_DIR};
use eth2_wallet::bip39::Seed;
use eth2_wallet::{recover_validator_secret_from_mnemonic, KeyType, ValidatorKeystores};
use std::path::PathBuf;
use validator_dir::Builder as ValidatorDirBuilder;
pub const CMD: &str = "recover";
pub const FIRST_INDEX_FLAG: &str = "first-index";
pub const MNEMONIC_FLAG: &str = "mnemonic-path";

pub fn cli_app() -> Command {
    Command::new(CMD)
        .about(
            "Recovers validator private keys given a BIP-39 mnemonic phrase. \
            If you did not specify a `--first-index` or count `--count`, by default this will \
            only recover the keys associated with the validator at index 0 for an HD wallet \
            in accordance with the EIP-2333 spec.")
        .arg(
            Arg::new(FIRST_INDEX_FLAG)
                .long(FIRST_INDEX_FLAG)
                .value_name("FIRST_INDEX")
                .help("The first of consecutive key indexes you wish to recover.")
                .action(ArgAction::Set)
                .required(false)
                .default_value("0")
                .display_order(0)
        )
        .arg(
            Arg::new(COUNT_FLAG)
                .long(COUNT_FLAG)
                .value_name("COUNT")
                .help("The number of validator keys you wish to recover. Counted consecutively from the provided `--first_index`.")
                .action(ArgAction::Set)
                .required(false)
                .default_value("1")
                .display_order(0)
        )
        .arg(
            Arg::new(MNEMONIC_FLAG)
                .long(MNEMONIC_FLAG)
                .value_name("MNEMONIC_PATH")
                .help(
                    "If present, the mnemonic will be read in from this file.",
                )
                .action(ArgAction::Set)
                .display_order(0)
        )
        .arg(
            Arg::new(SECRETS_DIR_FLAG)
                .long(SECRETS_DIR_FLAG)
                .value_name("SECRETS_DIR")
                .help(
                    "The path where the validator keystore passwords will be stored. \
                    Defaults to ~/.lighthouse/{network}/secrets",
                )
                .action(ArgAction::Set)
                .display_order(0)
        )
        .arg(
            Arg::new(STORE_WITHDRAW_FLAG)
                .long(STORE_WITHDRAW_FLAG)
                .help(
                    "If present, the withdrawal keystore will be stored alongside the voting \
                    keypair. It is generally recommended to *not* store the withdrawal key and \
                    instead generate them from the wallet seed when required.",
                )
                .action(ArgAction::SetTrue)
                .help_heading(FLAG_HEADER)
                .display_order(0)
        )
}

pub fn cli_run(matches: &ArgMatches, validator_dir: PathBuf) -> Result<(), String> {
    let secrets_dir = if matches.get_one::<String>("datadir").is_some() {
        let path: PathBuf = clap_utils::parse_required(matches, "datadir")?;
        path.join(DEFAULT_SECRET_DIR)
    } else {
        parse_path_or_default_with_flag(matches, SECRETS_DIR_FLAG, DEFAULT_SECRET_DIR)?
    };
    let first_index: u32 = clap_utils::parse_required(matches, FIRST_INDEX_FLAG)?;
    let count: u32 = clap_utils::parse_required(matches, COUNT_FLAG)?;
    let mnemonic_path: Option<PathBuf> = clap_utils::parse_optional(matches, MNEMONIC_FLAG)?;
    let stdin_inputs = cfg!(windows) || matches.get_flag(STDIN_INPUTS_FLAG);

    eprintln!("secrets-dir path: {:?}", secrets_dir);

    ensure_dir_exists(&validator_dir)?;
    ensure_dir_exists(&secrets_dir)?;

    eprintln!();
    eprintln!("WARNING: KEY RECOVERY CAN LEAD TO DUPLICATING VALIDATORS KEYS, WHICH CAN LEAD TO SLASHING.");
    eprintln!();

    let mnemonic = read_mnemonic_from_cli(mnemonic_path, stdin_inputs)?;

    let seed = Seed::new(&mnemonic, "");

    for index in first_index..first_index + count {
        let voting_password = random_password();
        let withdrawal_password = random_password();

        let derive = |key_type: KeyType, password: &[u8]| -> Result<Keystore, String> {
            let (secret, path) =
                recover_validator_secret_from_mnemonic(seed.as_bytes(), index, key_type)
                    .map_err(|e| format!("Unable to recover validator keys: {:?}", e))?;

            let keypair = keypair_from_secret(secret.as_bytes())
                .map_err(|e| format!("Unable build keystore: {:?}", e))?;

            KeystoreBuilder::new(&keypair, password, format!("{}", path))
                .map_err(|e| format!("Unable build keystore: {:?}", e))?
                .build()
                .map_err(|e| format!("Unable build keystore: {:?}", e))
        };

        let keystores = ValidatorKeystores {
            voting: derive(KeyType::Voting, voting_password.as_bytes())?,
            withdrawal: derive(KeyType::Withdrawal, withdrawal_password.as_bytes())?,
        };

        let voting_pubkey = keystores.voting.pubkey().to_string();

        ValidatorDirBuilder::new(validator_dir.clone())
            .password_dir(secrets_dir.clone())
            .voting_keystore(keystores.voting, voting_password.as_bytes())
            .withdrawal_keystore(keystores.withdrawal, withdrawal_password.as_bytes())
            .store_withdrawal_keystore(matches.get_flag(STORE_WITHDRAW_FLAG))
            .build()
            .map_err(|e| format!("Unable to build validator directory: {:?}", e))?;

        println!(
            "{}/{}\tIndex: {}\t0x{}",
            index - first_index + 1,
            count,
            index,
            voting_pubkey
        );
    }

    Ok(())
}

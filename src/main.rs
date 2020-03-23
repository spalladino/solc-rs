extern crate glob;
extern crate serde;
extern crate serde_json;

use glob::glob;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(serde::Serialize)]
struct SolidityFile {
    content: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SolidityArtifact {
    contract_name: String,
    file_name: String,
    source_path: String,
    source: String,
    bytecode: String,
    deployed_bytecode: String,
    source_map: String,
    deployed_source_map: String,
    abi: JsonValue,
    ast: JsonValue,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct SolcOutput {
    contracts: HashMap<String, HashMap<String, SolcContract>>,
    sources: HashMap<String, SolcSource>,
    errors: Vec<SolcError>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct SolcContract {
    evm: SolcContractEvm,
    abi: JsonValue,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SolcContractEvm {
    bytecode: SolcBytecodeOutput,
    deployed_bytecode: SolcBytecodeOutput,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SolcBytecodeOutput {
    object: String,
    source_map: String,
    // link_references,
    // opcodes,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct SolcSource {
    ast: JsonValue,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SolcError {
    severity: String,
    formatted_message: String,
}

fn build_contract_schemas(
    output: &SolcOutput,
    sources: &HashMap<String, SolidityFile>,
) -> Vec<SolidityArtifact> {
    output
        .contracts
        .iter()
        .flat_map(
            |(path, contracts): (&String, &HashMap<String, SolcContract>)| {
                let solc_source: &SolcSource = output.sources.get(path).unwrap();
                contracts
                    .iter()
                    .map(move |(name, contract): (&String, &SolcContract)| {
                        let ref source = sources.get(path).unwrap().content;
                        build_contract_schema(path, name, source, solc_source, contract)
                    })
            },
        )
        .collect()
}

fn build_contract_schema(
    path: &String,
    name: &String,
    source: &String,
    solc_source: &SolcSource,
    solc_contract: &SolcContract,
) -> SolidityArtifact {
    SolidityArtifact {
        abi: solc_contract.abi.clone(),
        bytecode: solc_contract.evm.bytecode.object.clone(),
        deployed_bytecode: solc_contract.evm.deployed_bytecode.object.clone(),
        contract_name: name.clone(),
        file_name: String::from(Path::new(path).file_name().unwrap().to_str().unwrap()),
        ast: solc_source.ast.clone(),
        source_path: path.clone(),
        source: source.clone(),
        source_map: solc_contract.evm.bytecode.source_map.clone(),
        deployed_source_map: solc_contract.evm.deployed_bytecode.source_map.clone(),
    }
}

fn write_contract_schemas(artifacts: &[SolidityArtifact], output_path: &Path) {
    for artifact in artifacts {
        let json =
            serde_json::to_string_pretty(artifact).expect("Error serializing solidity artifact");
        let mut path = PathBuf::from(output_path);
        path.push(&artifact.contract_name);
        path.set_extension("json");
        fs::write(path.as_path(), json).expect("Error writing solidity artifact");
    }
}

fn get_solidity_sources() -> HashMap<String, SolidityFile> {
    glob("./contracts/**/*.sol")
        .expect("Error parsing contracts glob")
        .map(|path: glob::GlobResult| {
            let path = path.expect("Error accessing local path");
            let content = fs::read_to_string(&path).expect("Error reading contract file");
            let filename = String::from(path.to_str().unwrap());
            (filename, SolidityFile { content })
        })
        .into_iter()
        .collect()
}

fn build_solc_input_json(
    sources: &HashMap<String, SolidityFile>,
    evm_version: &str,
) -> serde_json::Value {
    json!({
      "language": "Solidity",
      "settings": {
        "evmVersion": evm_version,
        "optimizer": {
          "enabled": false
        },
        "outputSelection": {
          "*": {
            "": ["ast"],
            "*": [
              "abi",
              "evm.bytecode.object",
              "evm.deployedBytecode.object",
            ],
          },
        }
      },
      "sources": sources
    })
}

pub fn main() {
    // Profiling
    let now = Instant::now();

    // Create list of solidity sources with content
    let sources: HashMap<String, SolidityFile> = get_solidity_sources();

    // Create standard-json input for solc
    let evm_version = "byzantium";
    let input = build_solc_input_json(&sources, &evm_version);

    // Compile & parse output
    let raw_output = solc::compile(&input.to_string());
    let output: SolcOutput = serde_json::from_str(&raw_output).unwrap();

    // Build & write artifacts
    let artifacts: Vec<SolidityArtifact> = build_contract_schemas(&output, &sources);
    let output_path = Path::new("./build/contracts/");
    fs::create_dir_all(output_path).expect("Error creating output dir");
    write_contract_schemas(&artifacts, &output_path);

    println!(
        "Compiled {} artifacts in {} seconds",
        artifacts.len(),
        now.elapsed().as_secs_f32()
    );
}

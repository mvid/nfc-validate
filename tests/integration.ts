import axios from "axios";
import { Wallet, SecretNetworkClient } from "secretjs";
import fs from "fs";
import assert from "assert";

// Returns a client with which we can interact with secret network
const initializeClient = async (endpoint: string, chainId: string) => {
  const wallet = new Wallet(); // Use default constructor of wallet to generate random mnemonic.
  const accAddress = wallet.address;
  const client = new SecretNetworkClient({
    // Create a client to interact with the network
    url: endpoint,
    chainId: chainId,
    wallet: wallet,
    walletAddress: accAddress,
  });

  console.log(`Initialized client with wallet address: ${accAddress}`);
  return client;
};

// Stores and instantiaties a new contract in our network
const initializeContract = async (
  client: SecretNetworkClient,
  contractPath: string
) => {
  const wasmCode = fs.readFileSync(contractPath);
  console.log("Uploading contract");

  const uploadReceipt = await client.tx.compute.storeCode(
    {
      wasm_byte_code: wasmCode,
      sender: client.address,
      source: "",
      builder: "",
    },
    {
      gasLimit: 5000000,
    }
  );

  if (uploadReceipt.code !== 0) {
    console.log(
      `Failed to get code id: ${JSON.stringify(uploadReceipt.rawLog)}`
    );
    throw new Error(`Failed to upload contract`);
  }

  const codeIdKv = uploadReceipt.jsonLog![0].events[0].attributes.find(
    (a: any) => {
      return a.key === "code_id";
    }
  );

  const codeId = Number(codeIdKv!.value);
  console.log("Contract codeId: ", codeId);

  const contractCodeHash = (await client.query.compute.codeHashByCodeId({code_id: String(codeId)})).code_hash;

  if (contractCodeHash === undefined) {
    throw new Error(`Failed to get code hash`);
  }

  console.log(`Contract hash: ${contractCodeHash}`);

  const contract = await client.tx.compute.instantiateContract(
    {
      sender: client.address,
      code_id: codeId,
      init_msg: { count: 4 }, // Initialize our counter to start from 4. This message will trigger our Init function
      code_hash: contractCodeHash,
      label: "My contract" + Math.ceil(Math.random() * 10000), // The label should be unique for every contract, add random string in order to maintain uniqueness
    },
    {
      gasLimit: 1000000,
    }
  );

  if (contract.code !== 0) {
    throw new Error(
      `Failed to instantiate the contract with the following error ${contract.rawLog}`
    );
  }

  const contractAddress = contract.arrayLog!.find(
    (log) => log.type === "message" && log.key === "contract_address"
  )!.value;

  console.log(`Contract address: ${contractAddress}`);

  const contractInfo: [string, string] = [contractCodeHash, contractAddress];
  return contractInfo;
};

const getFromFaucet = async (address: string) => {
  await axios.get(`http://localhost:5000/faucet?address=${address}`);
};

async function getScrtBalance(userCli: SecretNetworkClient): Promise<string> {
  let balanceResponse = await userCli.query.bank.balance({
    address: userCli.address,
    denom: "uscrt",
  });

  if (balanceResponse?.balance?.amount === undefined) {
    throw new Error(`Failed to get balance for address: ${userCli.address}`)
  }

  return balanceResponse.balance.amount;
}

async function fillUpFromFaucet(
  client: SecretNetworkClient,
  targetBalance: Number
) {
  let balance = await getScrtBalance(client);
  while (Number(balance) < targetBalance) {
    try {
      await getFromFaucet(client.address);
    } catch (e) {
      console.error(`failed to get tokens from faucet: ${e}`);
    }
    balance = await getScrtBalance(client);
  }
  console.error(`got tokens from faucet: ${balance}`);
}

// Initialization procedure
async function initializeAndUploadContract() {
  let endpoint = "http://localhost:1317";
  let chainId = "secretdev-1";

  const client = await initializeClient(endpoint, chainId);

  await fillUpFromFaucet(client, 100_000_000);

  const [contractHash, contractAddress] = await initializeContract(
    client,
    "contract.wasm"
  );

  var clientInfo: [SecretNetworkClient, string, string] = [
    client,
    contractHash,
    contractAddress,
  ];
  return clientInfo;
}


async function queryAdmin(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
): Promise<string> {
  type AdminResponse = { admin: string };

  const adminResponse = (await client.query.compute.queryContract({
    contract_address: contractAddress,
    code_hash: contractHash,
    query: { get_admin: {} },
  })) as AdminResponse;

  if ('err"' in adminResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(adminResponse)}`
    );
  }

  return adminResponse.admin;
}

async function incrementTx(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddess: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contract_address: contractAddess,
      code_hash: contractHash,
      msg: {
        increment: {},
      },
      sent_funds: [],
    },
    {
      gasLimit: 200000,
    }
  );

  //let parsedTransactionData = JSON.parse(fromUtf8(tx.data[0])); // In our case we don't really need to access transaction data
  console.log(`Increment TX used ${tx.gasUsed} gas`);
}

type Key = {
  value: Array<number>,
  version: number,
}

type NewTag = {
  id: number,
  change_key: Key,
  mac_read_key: Key,
};

async function registerTagTx(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddess: string,
  new_tag: NewTag,
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contract_address: contractAddess,
      code_hash: contractHash,
      msg: {
        register: {
          tag: new_tag,
        },
      },
      sent_funds: [],
    },
    {
      gasLimit: 200000,
    }
  );

  //let parsedTransactionData = JSON.parse(fromUtf8(tx.data[0])); // In our case we don't really need to access transaction data
  console.log(`Register Tag TX used ${tx.gasUsed} gas`);
}


// The following functions are only some examples of how to write integration tests, there are many tests that we might want to write here.
async function test_admin_on_intialization(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const onInitializationAdmin: string = await queryAdmin(
    client,
    contractHash,
    contractAddress
  );
  assert(
    onInitializationAdmin === client.address,
    `The admin on initialization expected to be ${client.address} instead of ${onInitializationAdmin}`
  );
}

async function test_increment_stress(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const onStartCounter: number = await queryCount(
    client,
    contractHash,
    contractAddress
  );

  let stressLoad: number = 10;
  for (let i = 0; i < stressLoad; ++i) {
    await incrementTx(client, contractHash, contractAddress);
  }

  const afterStressCounter: number = await queryCount(
    client,
    contractHash,
    contractAddress
  );
  assert(
    afterStressCounter - onStartCounter === stressLoad,
    `After running stress test the counter expected to be ${
      onStartCounter + 10
    } instead of ${afterStressCounter}`
  );
}

async function test_register_tag(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  let newTag: NewTag = {
    id: 1370400024739904,
    change_key: {
      value: [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
      version: 0,
    },
    mac_read_key: {
      value: [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
      version: 0,
    },
  };

  await registerTagTx(client, contractHash, contractAddress, newTag);
}

async function test_gas_limits() {
  // There is no accurate way to measure gas limits, but it is actually very recommended to make sure that the gas that is used by a specific tx makes sense
}

async function runTestFunction(
  tester: (
    client: SecretNetworkClient,
    contractHash: string,
    contractAddress: string
  ) => void,
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  console.log(`Testing ${tester.name}`);
  await tester(client, contractHash, contractAddress);
  console.log(`[SUCCESS] ${tester.name}`);
}

(async () => {
  const [client, contractHash, contractAddress] =
    await initializeAndUploadContract();

  await runTestFunction(
    test_admin_on_intialization,
    client,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    test_register_tag,
    client,
    contractHash,
    contractAddress
  );
  await runTestFunction(test_gas_limits, client, contractHash, contractAddress);
})();

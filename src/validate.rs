use cmac::{Cmac, Mac};
use aes::Aes128;
use std::convert::TryInto;
use cosmwasm_std::StdError;

// sv2 prefix 3cc300010080
const SV2_PREFIX: [u8; 6] = [0x3c, 0xc3, 0x00, 0x01, 0x00, 0x80];

pub fn build_sv2_message(uid: [u8; 7], count_lsb: [u8; 3]) -> [u8; 16] {
    let mut message: [u8; 16] = [0; 16];

    // set the message prefix
    for i in 0..6 {
        message[i] = SV2_PREFIX[i];
    }

    // append the uid
    for i in 0..7 {
        message[i + 6] = uid[i];
    }

    // append the count
    for i in 0..3 {
        message[i + 13] = count_lsb[i];
    }

    return message;
}

pub fn mac_message(key: [u8; 16], message: Vec<u8>) -> Result<Vec<u8>, StdError> {
    let mut mac = match Cmac::<Aes128>::new_from_slice(key.as_slice()) {
        Ok(m) => m,
        Err(e) => return Err(StdError::generic_err(e.to_string())),
    };
    mac.update(message.as_slice());
    let output = mac.finalize().into_bytes().to_vec();

    return Ok(output);
}


// verifies the Secure Unique NFC Message provided in the call
pub fn verify_sun(key: [u8; 16], uid: [u8; 7], count: [u8; 3], signature: [u8; 8]) -> Result<(), StdError> {
    // create initial sv2 message
    let sv2 = build_sv2_message(uid, count);
    // MAC the sv2 message with the mac read key
    let macd_sv2 = mac_message(key, sv2.to_vec())?;

    // use the mac'd message as the new key
    let macd_message_sized: [u8; 16] = macd_sv2.as_slice().try_into().expect("Cannot unpack MAC SV2 vector");
    let full_sun = mac_message(macd_message_sized, Vec::new())?;
    let truncated_sun: [u8; 8] = [full_sun[1], full_sun[3], full_sun[5], full_sun[7], full_sun[9], full_sun[11], full_sun[13], full_sun[15]];

    if truncated_sun != signature {
        return Err(StdError::generic_err("Provided signature is invalid"));
    }

    return Ok(());
}

use std::convert::TryInto;
use std::io::Read;
use cmac::{Cmac, Mac};
use aes::Aes128;
use cosmwasm_std::{Binary, Deps, DepsMut, entry_point, Env, MessageInfo, Response, StdError, StdResult, to_binary};
use cosmwasm_storage::PrefixedStorage;

use crate::msg::{AdminResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state;
use crate::state::{config, config_read, may_load, PREFIX_TAGS, save, State, Tag, u32_to_u8_3};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _: InstantiateMsg,
) -> StdResult<Response> {
    let state = State {
        admin: deps.api.addr_canonicalize(info.sender.as_str())?,
    };

    deps.api
        .debug(format!("Contract was initialized by {}", info.sender).as_str());
    config(deps.storage).save(&state)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(deps: DepsMut, _env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Register { tag } => try_register(deps, info, tag),
        ExecuteMsg::Validate { id, count, signature } => try_validate(deps, info, id, count, signature)
    }
}


pub fn try_register(deps: DepsMut, info: MessageInfo, tag: Tag) -> StdResult<Response> {
    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let state = config_read(deps.storage).load()?;


    if sender_address_raw != state.admin {
        return Err(StdError::generic_err("Only the contract admin can register new tags"));
    }

    let mut tag_store = PrefixedStorage::new(deps.storage, PREFIX_TAGS);
    let existing_tag: Option<Tag> = may_load(&tag_store, &tag.id)?;

    if existing_tag.is_some() {
        return Err(StdError::generic_err("Tag with ID already registered"));
    }

    // tag_store.set(tag.id.as_slice(), &Bincode2::serialize(&tag)?);
    save(&mut tag_store, tag.id.as_slice(), &tag)?;

    return Ok(Response::default());
}

// sv2 prefix 3cc300010080
const MAC_PREFIX: [u8; 6] = [0x3c, 0xc3, 0x00, 0x01, 0x00, 0x80];

fn build_sv2_message(uid: [u8; 7], count: [u8; 3]) -> [u8; 16] {
    let mut message: [u8; 16] = [0; 16];

    // set the message prefix
    for i in 0..6 {
        message[i] = MAC_PREFIX[i];
    }

    // append the uid
    for i in 0..7 {
        message[i + 6] = uid[i];
    }

    // append the count
    for i in 0..3 {
        message[i + 13] = count[i];
    }

    return message;
}

fn mac_message(key: [u8; 16], message: Vec<u8>) -> Result<Vec<u8>, StdError> {
    let mut mac = match Cmac::<Aes128>::new_from_slice(key.as_slice()) {
        Ok(m) => m,
        Err(e) => return Err(StdError::generic_err(e.to_string())),
    };
    mac.update(message.as_slice());
    let output = mac.finalize().into_bytes().to_vec();

    return Ok(output);
}

fn verify_mac(key: [u8; 16], message: [u8; 16], signature: [u8; 8]) -> Result<bool, StdError> {
    let mut mac = match Cmac::<Aes128>::new_from_slice(key.as_slice()) {
        Ok(m) => m,
        Err(e) => return Err(StdError::generic_err(e.to_string())),
    };
    mac.update(message.as_slice());

    match mac.verify_slice(signature.as_slice()) {
        Ok(_) => Ok(true),
        Err(_) => return Ok(false),
    }
}

pub fn try_validate(deps: DepsMut, _info: MessageInfo, tag_id: [u8; 7], count: u32, signature: [u8; 8]) -> StdResult<Response> {
    let mut tag_store = PrefixedStorage::new(deps.storage, PREFIX_TAGS);
    let mut tag: Tag = match may_load(&tag_store, &tag_id)? {
        Some(t) => t,
        None => return Err(StdError::generic_err("Tag with ID not found")),
    };

    // the tag counter is a u24. we need to make sure the value doesn't get exceeded
    if count >= 16_777_216 {
        return Err(StdError::generic_err("Count maximum has been exceeded"));
    }

    let last_tag_count = tag.count();
    // make sure the submission isn't older than last seen
    if last_tag_count >= count {
        return Err(StdError::generic_err("Count is older than latest seen"));
    }

    // validate the signature
    let message = build_sv2_message(tag.id, state::u32_to_u8_3(count));
    let valid = verify_mac(tag.mac_read_key.value, message, signature)?;
    if !valid {
        return Err(StdError::generic_err("Provided signature is invalid"));
    }

    // save the last seen tag count
    tag.count = u32_to_u8_3(count);
    save(&mut tag_store, tag_id.as_slice(), &tag)?;

    return Ok(Response::default());
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetAdmin => to_binary(&query_admin(deps)?),
    }
}

fn query_admin(deps: Deps) -> StdResult<AdminResponse> {
    let state = config_read(deps.storage).load()?;

    Ok(AdminResponse { admin: state.admin.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use cosmwasm_std::testing::*;
    use cosmwasm_std::{Coin, from_binary, Uint128};
    use crate::state::u32_to_u8_3;

    #[test]
    fn mac_calculation_spec() {
        //     based on https://www.nxp.com/docs/en/application-note/AN12196.pdf#page=24

        // key is 00000000000000000000000000000000
        let key: [u8; 16] = [0x00; 16];
        // count is 3D0000
        let count = base16::decode(b"3D0000").unwrap();
        // uid is 04DE5F1EACC040
        let uid = base16::decode(b"04DE5F1EACC040").unwrap();

        // let mut mac = Cmac::<Aes128>::new_from_slice(key.as_slice()).unwrap();

        let constructed_sv2_message = build_sv2_message(
            uid.as_slice().try_into().unwrap(),
            count.try_into().unwrap(),
        );

        // expected message is 3CC30001008004DE5F1EACC0403D0000
        let expected_sv2_message: [u8; 16] = [0x3C, 0xC3, 0x00, 0x01, 0x00, 0x80, 0x04, 0xDE, 0x5F, 0x1E, 0xAC, 0xC0, 0x40, 0x3D, 0x00, 0x00];
        assert_eq!(expected_sv2_message, constructed_sv2_message);

        // expected mac message is 3FB5F6E3A807A03D5E3570ACE393776F
        let expected_macd_message: [u8; 16] = [0x3F, 0xB5, 0xF6, 0xE3, 0xA8, 0x07, 0xA0, 0x3D, 0x5E, 0x35, 0x70, 0xAC, 0xE3, 0x93, 0x77, 0x6F];
        let macd_message = mac_message(key, constructed_sv2_message.to_vec()).unwrap();
        let macd_message_sized: [u8; 16] = macd_message.as_slice().try_into().unwrap();
        assert_eq!(expected_macd_message, macd_message_sized);

        let macd_full_sun = mac_message(macd_message_sized, Vec::new()).unwrap();
        let mut truncated_sun: [u8; 8] = [macd_full_sun[1], macd_full_sun[3], macd_full_sun[5], macd_full_sun[7], macd_full_sun[9], macd_full_sun[11], macd_full_sun[13], macd_full_sun[15]];

        // expected SUN message is 94EED9EE65337086
        let expected_sun: [u8; 8] = [0x94, 0xEE, 0xD9, 0xEE, 0x65, 0x33, 0x70, 0x86];
        assert_eq!(expected_sun, truncated_sun.as_slice());

        // let valid = verify_mac(key, constructed_sv2_message, expected_response.as_slice().try_into().unwrap()).unwrap();
        // assert!(valid);
    }

    #[test]
    fn mac_calculation() {
        // key is D83DFF5D173665B1CE275B33B9967EA9
        let key: [u8; 16] = [0xD8, 0x3D, 0xFF, 0x5D, 0x17, 0x36, 0x65, 0xB1, 0xCE, 0x27, 0x5B, 0x33, 0xB9, 0x96, 0x7E, 0xA9];
        let count = 13 as u32;
        let expected_response = base16::decode(b"89CB862EF84B069D").unwrap();
        let uid = base16::decode(b"048F6A2AAA6180").unwrap();

        // let mut mac = Cmac::<Aes128>::new_from_slice(key.as_slice()).unwrap();

        let constructed_message = build_sv2_message(
            uid.as_slice().try_into().unwrap(),
            u32_to_u8_3(count),
        );
        let valid = verify_mac(key, constructed_message, expected_response.as_slice().try_into().unwrap()).unwrap();
        assert!(valid);
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let info = mock_info(
            "creator",
            &[Coin {
                denom: "earth".to_string(),
                amount: Uint128::new(1000),
            }],
        );
        let init_msg = InstantiateMsg { count: 17 };

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, init_msg).unwrap();

        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAdmin {}).unwrap();
        let value: AdminResponse = from_binary(&res).unwrap();
        assert_eq!("creator", value.admin);
    }
}

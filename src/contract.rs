use bytes::{Bytes, BytesMut, BufMut};
use cmac::{Cmac, Mac};
use aes::Aes128;
use cosmwasm_std::{entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, StdError, Storage};
use cosmwasm_storage::PrefixedStorage;

use crate::msg::{AdminResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{config, config_read, may_load, save, State, Tag, PREFIX_TAGS};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
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
        ExecuteMsg::Register { tag} => try_register(deps, info, tag),
        ExecuteMsg::Validate { id, count, signature} => try_validate(deps, info,id, count, signature)
    }
}


pub fn try_register(deps: DepsMut, info: MessageInfo, tag: Tag ) -> StdResult<Response> {
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

fn build_mac_message(uid: [u8; 7], count: [u8; 3]) -> [u8; 16] {
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

fn u32_to_u8_3(input: u32) -> [u8; 3] {
    assert!(input < 16_777_216);

    let mut output: [u8; 3] = [0; 3];
    let input_bytes = input.to_be_bytes();
    for i in 0..=2 {
        output[i] = input_bytes[i + 1];
    }

    return output;
}

pub fn try_validate(deps: DepsMut, _info: MessageInfo, tag_id: [u8; 7], count: u32, signature: [u8; 8]) -> StdResult<Response> {
    let mut tag_store = PrefixedStorage::new(deps.storage, PREFIX_TAGS);
    let mut tag: Tag = match  may_load(&tag_store, &tag_id)? {
        Some(t) => t,
        None => return Err(StdError::generic_err("Tag with ID not found")),
    };

    // the tag counter is a u24. we need to make sure the value doesn't get exceeded
    if tag.count >= 16_777_216 {
        return Err(StdError::generic_err("Count maximum has been exceeded"));
    }

    // make sure the submission isn't older than last seen
    if tag.count >= count {
        return Err(StdError::generic_err("Count is older than latest seen"));
    }
    //
    // let mut mac = Cmac::<Aes128>::new_from_slice(
    //     tag.mac_read_key.value.as_slice()).unwrap();
    // /// SV2 = 3Ch || C3h || 00h || 01h || 00h || 80h [ || UID] [ || SDMReadCtr] [ || ZeroPadding]
    // /// SesSDMFileReadMACKey = MAC(SDMFileReadKey; SV2)
    //
    // // let sv2 = base16.decode("3cc300010080");
    // // mac.update();

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
    use std::convert::TryInto;
    use std::io::Read;
    // use core::slice::SlicePattern;
    use super::*;
    use cosmwasm_std::testing::*;
    use cosmwasm_std::{from_binary, Coin, Uint128};

    #[test]
    fn mac_calculation() {
        let key = base16::decode(b"D83DFF5D173665B1CE275B33B9967EA9").unwrap();
        let count = 13 as u32;
        let expected_response = b"89CB862EF84B069D";
        let uid = base16::decode(b"048F6A2AAA6180").unwrap();

        let mut mac = Cmac::<Aes128>::new_from_slice(key.as_slice()).unwrap();
        let sv2 = base16::decode("3cc300010080");

        let constructed_message = build_mac_message(
            uid.as_slice().try_into().unwrap(),
            u32_to_u8_3(count)
        );

        mac.update(constructed_message.as_slice());
        // assert_eq!(result.into_bytes(), expected_response);
        let output = mac.finalize();
        let output_bytes = output.into_bytes();
        let output_slice = output_bytes.as_slice();

        assert_eq!(expected_response, output_slice);
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

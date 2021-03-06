//
// Copyright (C) 2020 Signal Messenger, LLC.
// All rights reserved.
//
// SPDX-License-Identifier: GPL-3.0-only
//

mod support;

use libsignal_protocol_rust::*;
use rand::rngs::OsRng;
use std::convert::TryFrom;
use support::*;

#[test]
fn test_basic_prekey_v3() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let bob_pre_key_pair = KeyPair::generate(&mut csprng);
    let bob_signed_pre_key_pair = KeyPair::generate(&mut csprng);

    let bob_signed_pre_key_public = bob_signed_pre_key_pair.public_key.serialize();
    let bob_signed_pre_key_signature = bob_store
        .get_identity_key_pair(None)?
        .private_key()
        .calculate_signature(&bob_signed_pre_key_public, &mut csprng)?;

    let pre_key_id = 31337;
    let signed_pre_key_id = 22;

    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1,                                 // device id
        Some(pre_key_id),                  // pre key id
        Some(bob_pre_key_pair.public_key), // pre key
        signed_pre_key_id,                 // signed pre key id
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature.to_vec(),
        *bob_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    assert!(alice_store.load_session(&bob_address, None)?.is_some());
    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    let original_message = "L'homme est condamné à être libre";

    let outgoing_message = encrypt(&mut alice_store, &bob_address, original_message)?;

    assert_eq!(
        outgoing_message.message_type(),
        CiphertextMessageType::PreKey
    );

    let incoming_message = CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
        outgoing_message.serialize(),
    )?);

    bob_store.save_pre_key(
        pre_key_id,
        &PreKeyRecord::new(pre_key_id, &bob_pre_key_pair),
        None,
    )?;
    bob_store.save_signed_pre_key(
        signed_pre_key_id,
        &SignedPreKeyRecord::new(
            signed_pre_key_id,
            /*timestamp*/ 42,
            &bob_signed_pre_key_pair,
            &bob_signed_pre_key_signature,
        ),
        None,
    )?;

    let ptext = decrypt(&mut bob_store, &alice_address, &incoming_message)?;

    assert_eq!(String::from_utf8(ptext).unwrap(), original_message);

    let bobs_response = "Who watches the watchers?";

    assert!(bob_store.load_session(&alice_address, None)?.is_some());
    let bobs_session_with_alice = bob_store.load_session(&alice_address, None)?.unwrap();
    assert_eq!(
        bobs_session_with_alice.session_state()?.session_version()?,
        3
    );
    assert_eq!(
        bobs_session_with_alice
            .session_state()?
            .alice_base_key()?
            .len(),
        32 + 1
    );

    let bob_outgoing = encrypt(&mut bob_store, &alice_address, bobs_response)?;

    assert_eq!(bob_outgoing.message_type(), CiphertextMessageType::Whisper);

    let alice_decrypts = decrypt(&mut alice_store, &bob_address, &bob_outgoing)?;

    assert_eq!(String::from_utf8(alice_decrypts).unwrap(), bobs_response);

    run_interaction(
        &mut alice_store,
        &alice_address,
        &mut bob_store,
        &bob_address,
    )?;

    let mut alice_store = support::test_in_memory_protocol_store();

    let bob_pre_key_pair = KeyPair::generate(&mut csprng);
    let bob_signed_pre_key_pair = KeyPair::generate(&mut csprng);

    let bob_signed_pre_key_public = bob_signed_pre_key_pair.public_key.serialize();
    let bob_signed_pre_key_signature = bob_store
        .get_identity_key_pair(None)?
        .private_key()
        .calculate_signature(&bob_signed_pre_key_public, &mut csprng)?;

    let pre_key_id = 31337;
    let signed_pre_key_id = 22;

    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1, // device id
        Some(pre_key_id + 1),
        Some(bob_pre_key_pair.public_key), // pre key
        signed_pre_key_id + 1,
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature.to_vec(),
        *bob_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    bob_store.save_pre_key(
        pre_key_id + 1,
        &PreKeyRecord::new(pre_key_id + 1, &bob_pre_key_pair),
        None,
    )?;
    bob_store.save_signed_pre_key(
        signed_pre_key_id + 1,
        &SignedPreKeyRecord::new(
            signed_pre_key_id + 1,
            /*timestamp*/ 42,
            &bob_signed_pre_key_pair,
            &bob_signed_pre_key_signature,
        ),
        None,
    )?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    let outgoing_message = encrypt(&mut alice_store, &bob_address, original_message)?;

    assert_eq!(
        decrypt(&mut bob_store, &alice_address, &outgoing_message).unwrap_err(),
        SignalProtocolError::UntrustedIdentity(alice_address.clone())
    );

    assert_eq!(
        bob_store.save_identity(
            &alice_address,
            alice_store.get_identity_key_pair(None)?.identity_key(),
            None,
        )?,
        true
    );

    let decrypted = decrypt(&mut bob_store, &alice_address, &outgoing_message)?;
    assert_eq!(String::from_utf8(decrypted).unwrap(), original_message);

    // Sign pre-key with wrong key:
    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1, // device id
        Some(pre_key_id),
        Some(bob_pre_key_pair.public_key), // pre key
        signed_pre_key_id,
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature.to_vec(),
        *alice_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    assert!(process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )
    .is_err());

    Ok(())
}

#[test]
fn test_bad_signed_pre_key_signature() -> Result<(), SignalProtocolError> {
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let bob_store = support::test_in_memory_protocol_store();

    let mut csprng = OsRng;
    let bob_pre_key_pair = KeyPair::generate(&mut csprng);
    let bob_signed_pre_key_pair = KeyPair::generate(&mut csprng);

    let bob_signed_pre_key_public = bob_signed_pre_key_pair.public_key.serialize();
    let bob_signed_pre_key_signature = bob_store
        .get_identity_key_pair(None)?
        .private_key()
        .calculate_signature(&bob_signed_pre_key_public, &mut csprng)?
        .to_vec();

    let pre_key_id = 31337;
    let signed_pre_key_id = 22;

    for bit in 0..8 * bob_signed_pre_key_signature.len() {
        let mut bad_signature = bob_signed_pre_key_signature.clone();

        bad_signature[bit / 8] ^= 0x01u8 << (bit % 8);

        let bob_pre_key_bundle = PreKeyBundle::new(
            bob_store.get_local_registration_id(None)?,
            1,
            Some(pre_key_id),
            Some(bob_pre_key_pair.public_key),
            signed_pre_key_id,
            bob_signed_pre_key_pair.public_key,
            bad_signature,
            *bob_store.get_identity_key_pair(None)?.identity_key(),
        )?;

        assert!(process_prekey_bundle(
            &bob_address,
            &mut alice_store.session_store,
            &mut alice_store.identity_store,
            &bob_pre_key_bundle,
            &mut csprng,
            None,
        )
        .is_err());
    }

    // Finally check that the non-corrupted signature is accepted:

    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1,
        Some(pre_key_id),
        Some(bob_pre_key_pair.public_key),
        signed_pre_key_id,
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature,
        *bob_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    Ok(())
}

// testRepeatBundleMessageV2 cannot be represented

#[test]
fn repeat_bundle_message_v3() -> Result<(), SignalProtocolError> {
    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let mut csprng = OsRng;
    let bob_pre_key_pair = KeyPair::generate(&mut csprng);
    let bob_signed_pre_key_pair = KeyPair::generate(&mut csprng);

    let bob_signed_pre_key_public = bob_signed_pre_key_pair.public_key.serialize();
    let bob_signed_pre_key_signature = bob_store
        .get_identity_key_pair(None)?
        .private_key()
        .calculate_signature(&bob_signed_pre_key_public, &mut csprng)?;

    let pre_key_id = 31337;
    let signed_pre_key_id = 22;

    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1,                                 // device id
        Some(pre_key_id),                  // pre key id
        Some(bob_pre_key_pair.public_key), // pre key
        signed_pre_key_id,                 // signed pre key id
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature.to_vec(),
        *bob_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    assert!(alice_store.load_session(&bob_address, None)?.is_some());
    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    let original_message = "L'homme est condamné à être libre";

    let outgoing_message1 = encrypt(&mut alice_store, &bob_address, original_message)?;
    let outgoing_message2 = encrypt(&mut alice_store, &bob_address, original_message)?;

    assert_eq!(
        outgoing_message1.message_type(),
        CiphertextMessageType::PreKey
    );
    assert_eq!(
        outgoing_message2.message_type(),
        CiphertextMessageType::PreKey
    );

    let incoming_message = CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
        outgoing_message1.serialize(),
    )?);

    bob_store.save_pre_key(
        pre_key_id,
        &PreKeyRecord::new(pre_key_id, &bob_pre_key_pair),
        None,
    )?;
    bob_store.save_signed_pre_key(
        signed_pre_key_id,
        &SignedPreKeyRecord::new(
            signed_pre_key_id,
            /*timestamp*/ 42,
            &bob_signed_pre_key_pair,
            &bob_signed_pre_key_signature,
        ),
        None,
    )?;

    let ptext = decrypt(&mut bob_store, &alice_address, &incoming_message)?;
    assert_eq!(String::from_utf8(ptext).unwrap(), original_message);

    let bob_outgoing = encrypt(&mut bob_store, &alice_address, original_message)?;
    assert_eq!(bob_outgoing.message_type(), CiphertextMessageType::Whisper);
    let alice_decrypts = decrypt(&mut alice_store, &bob_address, &bob_outgoing)?;
    assert_eq!(String::from_utf8(alice_decrypts).unwrap(), original_message);

    // The test

    let incoming_message2 = CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
        outgoing_message2.serialize(),
    )?);

    let ptext = decrypt(&mut bob_store, &alice_address, &incoming_message2)?;
    assert_eq!(String::from_utf8(ptext).unwrap(), original_message);

    let bob_outgoing = encrypt(&mut bob_store, &alice_address, original_message)?;
    let alice_decrypts = decrypt(&mut alice_store, &bob_address, &bob_outgoing)?;
    assert_eq!(String::from_utf8(alice_decrypts).unwrap(), original_message);

    Ok(())
}

#[test]
fn bad_message_bundle() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let bob_pre_key_pair = KeyPair::generate(&mut csprng);
    let bob_signed_pre_key_pair = KeyPair::generate(&mut csprng);

    let bob_signed_pre_key_public = bob_signed_pre_key_pair.public_key.serialize();
    let bob_signed_pre_key_signature = bob_store
        .get_identity_key_pair(None)?
        .private_key()
        .calculate_signature(&bob_signed_pre_key_public, &mut csprng)?;

    let pre_key_id = 31337;
    let signed_pre_key_id = 22;

    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1,                                 // device id
        Some(pre_key_id),                  // pre key id
        Some(bob_pre_key_pair.public_key), // pre key
        signed_pre_key_id,                 // signed pre key id
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature.to_vec(),
        *bob_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    bob_store.save_pre_key(
        pre_key_id,
        &PreKeyRecord::new(pre_key_id, &bob_pre_key_pair),
        None,
    )?;
    bob_store.save_signed_pre_key(
        signed_pre_key_id,
        &SignedPreKeyRecord::new(
            signed_pre_key_id,
            /*timestamp*/ 42,
            &bob_signed_pre_key_pair,
            &bob_signed_pre_key_signature,
        ),
        None,
    )?;

    assert!(alice_store.load_session(&bob_address, None)?.is_some());
    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    let original_message = "L'homme est condamné à être libre";

    assert!(bob_store.get_pre_key(pre_key_id, None).is_ok());
    let outgoing_message = encrypt(&mut alice_store, &bob_address, original_message)?;

    assert_eq!(
        outgoing_message.message_type(),
        CiphertextMessageType::PreKey
    );

    let outgoing_message = outgoing_message.serialize().to_vec();

    let mut corrupted_message: Vec<u8> = outgoing_message.clone();
    corrupted_message[outgoing_message.len() - 10] ^= 1;

    let incoming_message = CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
        corrupted_message.as_slice(),
    )?);

    assert!(decrypt(&mut bob_store, &alice_address, &incoming_message).is_err());
    assert!(bob_store.get_pre_key(pre_key_id, None).is_ok());

    let incoming_message = CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
        outgoing_message.as_slice(),
    )?);

    let ptext = decrypt(&mut bob_store, &alice_address, &incoming_message)?;

    assert_eq!(String::from_utf8(ptext).unwrap(), original_message);
    assert_eq!(
        bob_store.get_pre_key(pre_key_id, None).unwrap_err(),
        SignalProtocolError::InvalidPreKeyId
    );

    Ok(())
}

#[test]
fn optional_one_time_prekey() -> Result<(), SignalProtocolError> {
    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let mut csprng = OsRng;
    let bob_signed_pre_key_pair = KeyPair::generate(&mut csprng);

    let bob_signed_pre_key_public = bob_signed_pre_key_pair.public_key.serialize();
    let bob_signed_pre_key_signature = bob_store
        .get_identity_key_pair(None)?
        .private_key()
        .calculate_signature(&bob_signed_pre_key_public, &mut csprng)?;

    let signed_pre_key_id = 22;

    let bob_pre_key_bundle = PreKeyBundle::new(
        bob_store.get_local_registration_id(None)?,
        1, // device id
        None,
        None,              // no pre key
        signed_pre_key_id, // signed pre key id
        bob_signed_pre_key_pair.public_key,
        bob_signed_pre_key_signature.to_vec(),
        *bob_store.get_identity_key_pair(None)?.identity_key(),
    )?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    let original_message = "L'homme est condamné à être libre";

    let outgoing_message = encrypt(&mut alice_store, &bob_address, original_message)?;

    assert_eq!(
        outgoing_message.message_type(),
        CiphertextMessageType::PreKey
    );

    let incoming_message = CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
        outgoing_message.serialize(),
    )?);

    bob_store.save_signed_pre_key(
        signed_pre_key_id,
        &SignedPreKeyRecord::new(
            signed_pre_key_id,
            /*timestamp*/ 42,
            &bob_signed_pre_key_pair,
            &bob_signed_pre_key_signature,
        ),
        None,
    )?;

    let ptext = decrypt(&mut bob_store, &alice_address, &incoming_message)?;

    assert_eq!(String::from_utf8(ptext).unwrap(), original_message);

    Ok(())
}

#[test]
fn basic_session_v3() -> Result<(), SignalProtocolError> {
    let (alice_session, bob_session) = initialize_sessions_v3()?;
    let alice_session_record = SessionRecord::new(alice_session);
    let bob_session_record = SessionRecord::new(bob_session);
    run_session_interaction(alice_session_record, bob_session_record)?;
    Ok(())
}

#[test]
fn message_key_limits() -> Result<(), SignalProtocolError> {
    let (alice_session, bob_session) = initialize_sessions_v3()?;
    let alice_session_record = SessionRecord::new(alice_session);
    let bob_session_record = SessionRecord::new(bob_session);

    let alice_address = ProtocolAddress::new("+14159999999".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14158888888".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    alice_store.store_session(&bob_address, &alice_session_record, None)?;
    bob_store.store_session(&alice_address, &bob_session_record, None)?;

    const MAX_MESSAGE_KEYS: usize = 2000; // same value as in library
    const TOO_MANY_MESSAGES: usize = MAX_MESSAGE_KEYS + 300;

    let mut inflight = Vec::with_capacity(TOO_MANY_MESSAGES);

    for i in 0..TOO_MANY_MESSAGES {
        inflight.push(encrypt(
            &mut alice_store,
            &bob_address,
            &format!("It's over {}", i),
        )?);
    }

    assert_eq!(
        String::from_utf8(decrypt(&mut bob_store, &alice_address, &inflight[1000])?).unwrap(),
        "It's over 1000"
    );
    assert_eq!(
        String::from_utf8(decrypt(
            &mut bob_store,
            &alice_address,
            &inflight[TOO_MANY_MESSAGES - 1]
        )?)
        .unwrap(),
        format!("It's over {}", TOO_MANY_MESSAGES - 1)
    );

    let err = decrypt(&mut bob_store, &alice_address, &inflight[5]).unwrap_err();
    assert_eq!(err, SignalProtocolError::DuplicatedMessage(2300, 5));
    Ok(())
}

fn run_session_interaction(
    alice_session: SessionRecord,
    bob_session: SessionRecord,
) -> Result<(), SignalProtocolError> {
    use rand::seq::SliceRandom;

    let alice_address = ProtocolAddress::new("+14159999999".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14158888888".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    alice_store.store_session(&bob_address, &alice_session, None)?;
    bob_store.store_session(&alice_address, &bob_session, None)?;

    let alice_plaintext = "This is Alice's message";
    let alice_ciphertext = encrypt(&mut alice_store, &bob_address, alice_plaintext)?;
    let bob_decrypted = decrypt(&mut bob_store, &alice_address, &alice_ciphertext)?;
    assert_eq!(String::from_utf8(bob_decrypted).unwrap(), alice_plaintext);

    let bob_plaintext = "This is Bob's reply";

    let bob_ciphertext = encrypt(&mut bob_store, &alice_address, bob_plaintext)?;
    let alice_decrypted = decrypt(&mut alice_store, &bob_address, &bob_ciphertext)?;
    assert_eq!(String::from_utf8(alice_decrypted).unwrap(), bob_plaintext);

    const ALICE_MESSAGE_COUNT: usize = 50;
    const BOB_MESSAGE_COUNT: usize = 50;

    let mut alice_messages = Vec::with_capacity(ALICE_MESSAGE_COUNT);

    for i in 0..ALICE_MESSAGE_COUNT {
        let ptext = format!("смерть за смерть {}", i);
        let ctext = encrypt(&mut alice_store, &bob_address, &ptext)?;
        alice_messages.push((ptext, ctext));
    }

    let mut rng = rand::rngs::OsRng;

    alice_messages.shuffle(&mut rng);

    for i in 0..ALICE_MESSAGE_COUNT / 2 {
        let ptext = decrypt(&mut bob_store, &alice_address, &alice_messages[i].1)?;
        assert_eq!(String::from_utf8(ptext).unwrap(), alice_messages[i].0);
    }

    let mut bob_messages = Vec::with_capacity(BOB_MESSAGE_COUNT);

    for i in 0..BOB_MESSAGE_COUNT {
        let ptext = format!("Relax in the safety of your own delusions. {}", i);
        let ctext = encrypt(&mut bob_store, &alice_address, &ptext)?;
        bob_messages.push((ptext, ctext));
    }

    bob_messages.shuffle(&mut rng);

    for i in 0..BOB_MESSAGE_COUNT / 2 {
        let ptext = decrypt(&mut alice_store, &bob_address, &bob_messages[i].1)?;
        assert_eq!(String::from_utf8(ptext).unwrap(), bob_messages[i].0);
    }

    for i in ALICE_MESSAGE_COUNT / 2..ALICE_MESSAGE_COUNT {
        let ptext = decrypt(&mut bob_store, &alice_address, &alice_messages[i].1)?;
        assert_eq!(String::from_utf8(ptext).unwrap(), alice_messages[i].0);
    }

    for i in BOB_MESSAGE_COUNT / 2..BOB_MESSAGE_COUNT {
        let ptext = decrypt(&mut alice_store, &bob_address, &bob_messages[i].1)?;
        assert_eq!(String::from_utf8(ptext).unwrap(), bob_messages[i].0);
    }

    Ok(())
}

fn run_interaction(
    alice_store: &mut InMemSignalProtocolStore,
    alice_address: &ProtocolAddress,
    bob_store: &mut InMemSignalProtocolStore,
    bob_address: &ProtocolAddress,
) -> Result<(), SignalProtocolError> {
    let alice_ptext = "It's rabbit season";

    let alice_message = encrypt(alice_store, bob_address, alice_ptext)?;
    assert_eq!(alice_message.message_type(), CiphertextMessageType::Whisper);
    assert_eq!(
        String::from_utf8(decrypt(bob_store, alice_address, &alice_message)?).unwrap(),
        alice_ptext
    );

    let bob_ptext = "It's duck season";

    let bob_message = encrypt(bob_store, alice_address, bob_ptext)?;
    assert_eq!(bob_message.message_type(), CiphertextMessageType::Whisper);
    assert_eq!(
        String::from_utf8(decrypt(alice_store, bob_address, &bob_message)?).unwrap(),
        bob_ptext
    );

    for i in 0..10 {
        let alice_ptext = format!("A->B message {}", i);
        let alice_message = encrypt(alice_store, bob_address, &alice_ptext)?;
        assert_eq!(alice_message.message_type(), CiphertextMessageType::Whisper);
        assert_eq!(
            String::from_utf8(decrypt(bob_store, alice_address, &alice_message)?).unwrap(),
            alice_ptext
        );
    }

    for i in 0..10 {
        let bob_ptext = format!("B->A message {}", i);
        let bob_message = encrypt(bob_store, alice_address, &bob_ptext)?;
        assert_eq!(bob_message.message_type(), CiphertextMessageType::Whisper);
        assert_eq!(
            String::from_utf8(decrypt(alice_store, bob_address, &bob_message)?).unwrap(),
            bob_ptext
        );
    }

    let mut alice_ooo_messages = vec![];

    for i in 0..10 {
        let alice_ptext = format!("A->B OOO message {}", i);
        let alice_message = encrypt(alice_store, bob_address, &alice_ptext)?;
        alice_ooo_messages.push((alice_ptext, alice_message));
    }

    for i in 0..10 {
        let alice_ptext = format!("A->B post-OOO message {}", i);
        let alice_message = encrypt(alice_store, bob_address, &alice_ptext)?;
        assert_eq!(alice_message.message_type(), CiphertextMessageType::Whisper);
        assert_eq!(
            String::from_utf8(decrypt(bob_store, alice_address, &alice_message)?).unwrap(),
            alice_ptext
        );
    }

    for i in 0..10 {
        let bob_ptext = format!("B->A message post-OOO {}", i);
        let bob_message = encrypt(bob_store, alice_address, &bob_ptext)?;
        assert_eq!(bob_message.message_type(), CiphertextMessageType::Whisper);
        assert_eq!(
            String::from_utf8(decrypt(alice_store, bob_address, &bob_message)?).unwrap(),
            bob_ptext
        );
    }

    for (ptext, ctext) in alice_ooo_messages {
        assert_eq!(
            String::from_utf8(decrypt(bob_store, alice_address, &ctext)?).unwrap(),
            ptext
        );
    }

    Ok(())
}

fn is_session_id_equal(
    alice_store: &dyn ProtocolStore,
    alice_address: &ProtocolAddress,
    bob_store: &dyn ProtocolStore,
    bob_address: &ProtocolAddress,
) -> Result<bool, SignalProtocolError> {
    Ok(alice_store
        .load_session(bob_address, None)?
        .unwrap()
        .session_state()?
        .alice_base_key()
        .clone()
        == bob_store
            .load_session(alice_address, None)?
            .unwrap()
            .session_state()?
            .alice_base_key()
            .clone())
}

#[test]
fn basic_simultaneous_initiate() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let alice_pre_key_bundle = create_pre_key_bundle(&mut alice_store, &mut csprng)?;
    let bob_pre_key_bundle = create_pre_key_bundle(&mut bob_store, &mut csprng)?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    process_prekey_bundle(
        &alice_address,
        &mut bob_store.session_store,
        &mut bob_store.identity_store,
        &alice_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
    let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

    assert_eq!(
        message_for_bob.message_type(),
        CiphertextMessageType::PreKey
    );
    assert_eq!(
        message_for_alice.message_type(),
        CiphertextMessageType::PreKey
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let alice_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            message_for_alice.serialize(),
        )?),
    )?;
    assert_eq!(String::from_utf8(alice_plaintext).unwrap(), "hi alice");

    let bob_plaintext = decrypt(
        &mut bob_store,
        &alice_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            message_for_bob.serialize(),
        )?),
    )?;
    assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );
    assert_eq!(
        bob_store
            .load_session(&alice_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let alice_response = encrypt(&mut alice_store, &bob_address, "nice to see you")?;

    assert_eq!(
        alice_response.message_type(),
        CiphertextMessageType::Whisper
    );

    let response_plaintext = decrypt(
        &mut bob_store,
        &alice_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(alice_response.serialize())?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "nice to see you"
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        true
    );

    let bob_response = encrypt(&mut bob_store, &alice_address, "you as well")?;

    assert_eq!(bob_response.message_type(), CiphertextMessageType::Whisper);

    let response_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(bob_response.serialize())?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "you as well"
    );

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        true
    );

    Ok(())
}

#[test]
fn simultaneous_initiate_with_lossage() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let alice_pre_key_bundle = create_pre_key_bundle(&mut alice_store, &mut csprng)?;
    let bob_pre_key_bundle = create_pre_key_bundle(&mut bob_store, &mut csprng)?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    process_prekey_bundle(
        &alice_address,
        &mut bob_store.session_store,
        &mut bob_store.identity_store,
        &alice_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
    let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

    assert_eq!(
        message_for_bob.message_type(),
        CiphertextMessageType::PreKey
    );
    assert_eq!(
        message_for_alice.message_type(),
        CiphertextMessageType::PreKey
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let bob_plaintext = decrypt(
        &mut bob_store,
        &alice_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            message_for_bob.serialize(),
        )?),
    )?;
    assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );
    assert_eq!(
        bob_store
            .load_session(&alice_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    let alice_response = encrypt(&mut alice_store, &bob_address, "nice to see you")?;

    assert_eq!(alice_response.message_type(), CiphertextMessageType::PreKey);

    let response_plaintext = decrypt(
        &mut bob_store,
        &alice_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            alice_response.serialize(),
        )?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "nice to see you"
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        true
    );

    let bob_response = encrypt(&mut bob_store, &alice_address, "you as well")?;

    assert_eq!(bob_response.message_type(), CiphertextMessageType::Whisper);

    let response_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(bob_response.serialize())?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "you as well"
    );

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        true
    );

    Ok(())
}

#[test]
fn simultaneous_initiate_lost_message() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let alice_pre_key_bundle = create_pre_key_bundle(&mut alice_store, &mut csprng)?;
    let bob_pre_key_bundle = create_pre_key_bundle(&mut bob_store, &mut csprng)?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    process_prekey_bundle(
        &alice_address,
        &mut bob_store.session_store,
        &mut bob_store.identity_store,
        &alice_pre_key_bundle,
        &mut csprng,
        None,
    )?;

    let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
    let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

    assert_eq!(
        message_for_bob.message_type(),
        CiphertextMessageType::PreKey
    );
    assert_eq!(
        message_for_alice.message_type(),
        CiphertextMessageType::PreKey
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let alice_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            message_for_alice.serialize(),
        )?),
    )?;
    assert_eq!(String::from_utf8(alice_plaintext).unwrap(), "hi alice");

    let bob_plaintext = decrypt(
        &mut bob_store,
        &alice_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            message_for_bob.serialize(),
        )?),
    )?;
    assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

    assert_eq!(
        alice_store
            .load_session(&bob_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );
    assert_eq!(
        bob_store
            .load_session(&alice_address, None)?
            .unwrap()
            .session_state()?
            .session_version()?,
        3
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let alice_response = encrypt(&mut alice_store, &bob_address, "nice to see you")?;

    assert_eq!(
        alice_response.message_type(),
        CiphertextMessageType::Whisper
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let bob_response = encrypt(&mut bob_store, &alice_address, "you as well")?;

    assert_eq!(bob_response.message_type(), CiphertextMessageType::Whisper);

    let response_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(bob_response.serialize())?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "you as well"
    );

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        true
    );

    Ok(())
}

#[test]
fn simultaneous_initiate_repeated_messages() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    for _ in 0..15 {
        let alice_pre_key_bundle = create_pre_key_bundle(&mut alice_store, &mut csprng)?;
        let bob_pre_key_bundle = create_pre_key_bundle(&mut bob_store, &mut csprng)?;

        process_prekey_bundle(
            &bob_address,
            &mut alice_store.session_store,
            &mut alice_store.identity_store,
            &bob_pre_key_bundle,
            &mut csprng,
            None,
        )?;

        process_prekey_bundle(
            &alice_address,
            &mut bob_store.session_store,
            &mut bob_store.identity_store,
            &alice_pre_key_bundle,
            &mut csprng,
            None,
        )?;

        let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
        let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

        assert_eq!(
            message_for_bob.message_type(),
            CiphertextMessageType::PreKey
        );
        assert_eq!(
            message_for_alice.message_type(),
            CiphertextMessageType::PreKey
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );

        let alice_plaintext = decrypt(
            &mut alice_store,
            &bob_address,
            &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
                message_for_alice.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(alice_plaintext).unwrap(), "hi alice");

        let bob_plaintext = decrypt(
            &mut bob_store,
            &alice_address,
            &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
                message_for_bob.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

        assert_eq!(
            alice_store
                .load_session(&bob_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );
        assert_eq!(
            bob_store
                .load_session(&alice_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );
    }

    for _ in 0..50 {
        let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
        let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

        assert_eq!(
            message_for_bob.message_type(),
            CiphertextMessageType::Whisper
        );
        assert_eq!(
            message_for_alice.message_type(),
            CiphertextMessageType::Whisper
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );

        let alice_plaintext = decrypt(
            &mut alice_store,
            &bob_address,
            &CiphertextMessage::SignalMessage(SignalMessage::try_from(
                message_for_alice.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(alice_plaintext).unwrap(), "hi alice");

        let bob_plaintext = decrypt(
            &mut bob_store,
            &alice_address,
            &CiphertextMessage::SignalMessage(SignalMessage::try_from(
                message_for_bob.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

        assert_eq!(
            alice_store
                .load_session(&bob_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );
        assert_eq!(
            bob_store
                .load_session(&alice_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );
    }

    let alice_response = encrypt(&mut alice_store, &bob_address, "nice to see you")?;

    assert_eq!(
        alice_response.message_type(),
        CiphertextMessageType::Whisper
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let bob_response = encrypt(&mut bob_store, &alice_address, "you as well")?;

    assert_eq!(bob_response.message_type(), CiphertextMessageType::Whisper);

    let response_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(bob_response.serialize())?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "you as well"
    );

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        true
    );

    Ok(())
}

#[test]
fn simultaneous_initiate_lost_message_repeated_messages() -> Result<(), SignalProtocolError> {
    let mut csprng = OsRng;

    let alice_address = ProtocolAddress::new("+14151111111".to_owned(), 1);
    let bob_address = ProtocolAddress::new("+14151111112".to_owned(), 1);

    let mut alice_store = support::test_in_memory_protocol_store();
    let mut bob_store = support::test_in_memory_protocol_store();

    let bob_pre_key_bundle = create_pre_key_bundle(&mut bob_store, &mut csprng)?;

    process_prekey_bundle(
        &bob_address,
        &mut alice_store.session_store,
        &mut alice_store.identity_store,
        &bob_pre_key_bundle,
        &mut csprng,
        None,
    )?;
    let lost_message_for_bob = encrypt(&mut alice_store, &bob_address, "it was so long ago")?;

    for _ in 0..15 {
        let alice_pre_key_bundle = create_pre_key_bundle(&mut alice_store, &mut csprng)?;
        let bob_pre_key_bundle = create_pre_key_bundle(&mut bob_store, &mut csprng)?;

        process_prekey_bundle(
            &bob_address,
            &mut alice_store.session_store,
            &mut alice_store.identity_store,
            &bob_pre_key_bundle,
            &mut csprng,
            None,
        )?;

        process_prekey_bundle(
            &alice_address,
            &mut bob_store.session_store,
            &mut bob_store.identity_store,
            &alice_pre_key_bundle,
            &mut csprng,
            None,
        )?;

        let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
        let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

        assert_eq!(
            message_for_bob.message_type(),
            CiphertextMessageType::PreKey
        );
        assert_eq!(
            message_for_alice.message_type(),
            CiphertextMessageType::PreKey
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );

        let alice_plaintext = decrypt(
            &mut alice_store,
            &bob_address,
            &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
                message_for_alice.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(alice_plaintext).unwrap(), "hi alice");

        let bob_plaintext = decrypt(
            &mut bob_store,
            &alice_address,
            &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
                message_for_bob.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

        assert_eq!(
            alice_store
                .load_session(&bob_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );
        assert_eq!(
            bob_store
                .load_session(&alice_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );
    }

    for _ in 0..50 {
        let message_for_bob = encrypt(&mut alice_store, &bob_address, "hi bob")?;
        let message_for_alice = encrypt(&mut bob_store, &alice_address, "hi alice")?;

        assert_eq!(
            message_for_bob.message_type(),
            CiphertextMessageType::Whisper
        );
        assert_eq!(
            message_for_alice.message_type(),
            CiphertextMessageType::Whisper
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );

        let alice_plaintext = decrypt(
            &mut alice_store,
            &bob_address,
            &CiphertextMessage::SignalMessage(SignalMessage::try_from(
                message_for_alice.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(alice_plaintext).unwrap(), "hi alice");

        let bob_plaintext = decrypt(
            &mut bob_store,
            &alice_address,
            &CiphertextMessage::SignalMessage(SignalMessage::try_from(
                message_for_bob.serialize(),
            )?),
        )?;
        assert_eq!(String::from_utf8(bob_plaintext).unwrap(), "hi bob");

        assert_eq!(
            alice_store
                .load_session(&bob_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );
        assert_eq!(
            bob_store
                .load_session(&alice_address, None)?
                .unwrap()
                .session_state()?
                .session_version()?,
            3
        );

        assert_eq!(
            is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
            false
        );
    }

    let alice_response = encrypt(&mut alice_store, &bob_address, "nice to see you")?;

    assert_eq!(
        alice_response.message_type(),
        CiphertextMessageType::Whisper
    );

    assert_eq!(
        is_session_id_equal(&alice_store, &alice_address, &bob_store, &bob_address)?,
        false
    );

    let bob_response = encrypt(&mut bob_store, &alice_address, "you as well")?;

    assert_eq!(bob_response.message_type(), CiphertextMessageType::Whisper);

    let response_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(bob_response.serialize())?),
    )?;
    assert_eq!(
        String::from_utf8(response_plaintext).unwrap(),
        "you as well"
    );

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        true
    );

    let blast_from_the_past = decrypt(
        &mut bob_store,
        &alice_address,
        &CiphertextMessage::PreKeySignalMessage(PreKeySignalMessage::try_from(
            lost_message_for_bob.serialize(),
        )?),
    )?;
    assert_eq!(
        String::from_utf8(blast_from_the_past).unwrap(),
        "it was so long ago"
    );

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        false
    );

    let bob_response = encrypt(&mut bob_store, &alice_address, "so it was")?;

    assert_eq!(bob_response.message_type(), CiphertextMessageType::Whisper);

    let response_plaintext = decrypt(
        &mut alice_store,
        &bob_address,
        &CiphertextMessage::SignalMessage(SignalMessage::try_from(bob_response.serialize())?),
    )?;
    assert_eq!(String::from_utf8(response_plaintext).unwrap(), "so it was");

    assert_eq!(
        is_session_id_equal(&bob_store, &bob_address, &alice_store, &alice_address)?,
        true
    );

    Ok(())
}

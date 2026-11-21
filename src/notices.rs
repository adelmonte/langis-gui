//! What timeline notices say, in Signal Desktop's words. Ported from
//! Vernam's `views/notices.rs` (AGPL-3.0, Ken Powers) without its UI parts.

use langis_store::{GroupNotice, Notice};

use crate::format::timer_long;
use crate::i18n::tr;

/// `name(sid)` gives a person's display name (`sid` is never `None`).
pub fn notice_text(notice: &Notice, name: impl Fn(&str) -> String) -> String {
    match notice {
        Notice::Timer { by: None, seconds: 0 } => tr("You disabled disappearing messages.").into(),
        Notice::Timer { by: Some(by), seconds: 0 } => crate::i18n::tf("icu:disabledDisappearingMessages", &format!("{} disabled disappearing messages.", name(by)), &[("name", crate::i18n::Arg::from(&name(by)))]),
        Notice::Timer { by: None, seconds } => {
            crate::i18n::tf("icu:youChangedTheTimer", &format!("You set the disappearing message time to {}.", timer_long(*seconds)), &[("time", crate::i18n::Arg::from(&timer_long(*seconds)))])
        }
        Notice::Timer { by: Some(by), seconds } => {
            crate::i18n::tf("icu:theyChangedTheTimer", &format!("{} set the disappearing message time to {}.", name(by), timer_long(*seconds)), &[("name", crate::i18n::Arg::from(&name(by))), ("time", crate::i18n::Arg::from(&timer_long(*seconds)))])
        }
        Notice::Pinned { by: None, .. } => tr("You pinned a message").into(),
        Notice::Pinned { by: Some(by), .. } => crate::i18n::tf("icu:PinnedMessageNotification__Message--SomeoneElse", &format!("{} pinned a message", name(by)), &[("sender", crate::i18n::Arg::from(&name(by)))]),
        Notice::Group { by, change } => group_text(by.as_deref(), change, name),
        Notice::SafetyNumberChanged { who } => format!("Your safety number with {} has changed.", name(who)),
        Notice::Call { incoming, video, outcome, .. } => call_text(*incoming, *video, *outcome),
        notice @ Notice::GroupCall { by, status, .. } => group_call_text(notice.group_call_incoming(), by.as_deref(), *status, name),
        // Desktop's `icu:PollTerminate--you` / `--other`.
        Notice::PollEnded { by: None, question, .. } => crate::i18n::tf("icu:PollTerminate--you", &format!("You ended the poll: {question}"), &[("poll", crate::i18n::Arg::from(&question))]),
        Notice::PollEnded { by: Some(by), question, .. } => crate::i18n::tf("icu:PollTerminate--other", &format!("{} ended the poll: {question}", name(by)), &[("name", crate::i18n::Arg::from(&name(by))), ("poll", crate::i18n::Arg::from(&question))]),
        Notice::MessageRequest { event, group, who } => message_request_text(*event, *group, who.as_deref(), name),
        // Without their address book name; see `profile_change_text`.
        Notice::ProfileNameChanged { old_name, new_name, .. } => profile_change_text(old_name, new_name, None),
        // Desktop's `icu:ChangeNumber--notification`.
        Notice::NumberChanged { who } => crate::i18n::tf("icu:ChangeNumber--notification", &format!("{} changed their phone number", name(who)), &[("sender", crate::i18n::Arg::from(&name(who)))]),
        // `icu:ChatRefresh--notification`.
        Notice::SessionRefreshed => tr("Chat session refreshed").into(),
        // `icu:ContactRemovedNotification__text`.
        Notice::ContactRemoved => tr("You have removed this person, messaging them again will add them back to your list.").into(),
        // `icu:JoinedSignal--notification`.
        Notice::JoinedSignal { .. } => tr("Contact joined Signal").into(),
        Notice::SessionEnded => tr("Secure session reset").into(),
        Notice::Merged { who, phone: Some(phone) } => crate::i18n::tf("icu:ConversationMerge--notification--with-e164", &format!("Your message history with {} and their number {phone} has been merged.", name(who)), &[("conversationTitle", crate::i18n::Arg::from(&name(who))), ("obsoleteConversationNumber", crate::i18n::Arg::from(&phone))]),
        Notice::Merged { who, phone: None } => crate::i18n::tf("icu:ConversationMerge--notification--no-title", &format!("Your message history with {} and another chat that belonged to them has been merged.", name(who)), &[("conversationTitle", crate::i18n::Arg::from(&name(who)))]),
        Notice::NumberIsTheirs { who, phone } => crate::i18n::tf("icu:PhoneNumberDiscovery--notification--noSharedGroup", &format!("{phone} belongs to {}", name(who)), &[("phoneNumber", crate::i18n::Arg::from(&phone)), ("conversationTitle", crate::i18n::Arg::from(&name(who)))]),
        // `icu:TitleTransition--notification`.
        Notice::StartedWith { previous, .. } => crate::i18n::tf("icu:TitleTransition--notification", &format!("You started this chat with {previous}"), &[("oldTitle", crate::i18n::Arg::from(&previous))]),
        // `icu:DeliveryIssue--notification`.
        Notice::DeliveryIssue { who } => format!("A message from {} couldn\u{2019}t be delivered", name(who)),
        // `icu:Message--unsupported-message` and `--from-me-unsupported-message`.
        Notice::UnsupportedMessage { who: Some(who) } => crate::i18n::tf("icu:Message--unsupported-message", &format!(
            "{} sent you a message that can't be processed or displayed because it uses a new Signal feature.",
            name(who)
        ), &[("contact", crate::i18n::Arg::from(&name(who)))]),
        Notice::UnsupportedMessage { who: None } => {
            tr("One of your devices sent a message that can't be processed or displayed because it uses a new Signal feature.").into()
        }
        // `icu:youMarkedAsVerified` and the rest (`VerificationNotification`).
        Notice::Verified { who, verified, local } => format!(
            "You marked your Safety Number with {} as {}{}",
            name(who),
            if *verified { "verified" } else { "not verified" },
            if *local { "" } else { " from another device" }
        ),
    }
}

/// Desktop's `getStringForProfileChange`: `icu:contactChangedProfileName`
/// with the name we know them by (`sender`, when we saved a name for them),
/// else `icu:changedProfileName`.
pub fn profile_change_text(old_name: &str, new_name: &str, sender: Option<&str>) -> String {
    match sender {
        Some(sender) => crate::i18n::tf("icu:contactChangedProfileName", &format!("{sender} changed their profile name from {old_name} to {new_name}."), &[("sender", crate::i18n::Arg::from(&sender)), ("oldProfile", crate::i18n::Arg::from(&old_name)), ("newProfile", crate::i18n::Arg::from(&new_name))]),
        None => crate::i18n::tf("icu:changedProfileName", &format!("{old_name} changed their profile name to {new_name}."), &[("oldProfile", crate::i18n::Arg::from(&old_name)), ("newProfile", crate::i18n::Arg::from(&new_name))]),
    }
}

fn message_request_text(event: langis_store::MessageRequestEvent, group: bool, who: Option<&str>, name: impl Fn(&str) -> String) -> String {
    use langis_store::MessageRequestEvent;
    match (event, group, who) {
        (MessageRequestEvent::Accept, false, Some(who)) => crate::i18n::tf("icu:MessageRequestResponseNotification__Message--Accepted-Contact-Title", &format!("You accepted {}'s message request", name(who)), &[("userProfileName", crate::i18n::Arg::from(&name(who)))]),
        (MessageRequestEvent::Accept, _, _) => tr("You accepted the message request").into(),
        (MessageRequestEvent::Block, true, _) => tr("You blocked the group").into(),
        (MessageRequestEvent::Block, false, _) => tr("You blocked this person").into(),
        (MessageRequestEvent::Unblock, true, _) => tr("You unblocked the group").into(),
        (MessageRequestEvent::Unblock, false, _) => tr("You unblocked this person").into(),
        (MessageRequestEvent::Spam, _, _) => tr("Reported as spam").into(),
    }
}

/// A group call's notice, after Desktop's `getGroupCallNotificationText`:
/// one rung and missed or declined says so; otherwise who started it.
/// (Desktop also says "The video call has ended" once it's over, which
/// needs the call's live state; this doesn't have it.)
fn group_call_text(
    incoming: bool,
    by: Option<&str>,
    status: Option<langis_store::GroupCallStatus>,
    name: impl Fn(&str) -> String,
) -> String {
    use langis_store::GroupCallStatus;
    match (incoming, status, by) {
        (true, Some(GroupCallStatus::Missed), _) => "Missed group call".into(),
        (true, Some(GroupCallStatus::Declined), _) => "Declined group call".into(),
        (false, _, _) => "You started a group call".into(),
        (true, _, Some(by)) => format!("{} started a group call", name(by)),
        (true, _, None) => "A group call was started".into(),
    }
}

/// Desktop's call history wording (`icu:missedIncomingAudioCall` and the rest).
pub fn call_text(incoming: bool, video: bool, outcome: langis_store::CallOutcome) -> String {
    use langis_store::CallOutcome;
    let kind = if video { "video" } else { "voice" };
    match outcome {
        CallOutcome::Accepted if incoming => format!("Incoming {kind} call"),
        CallOutcome::Accepted => format!("Outgoing {kind} call"),
        CallOutcome::Missed => format!("Missed {kind} call"),
        CallOutcome::Declined => format!("Declined {kind} call"),
        CallOutcome::Unanswered => format!("Unanswered {kind} call"),
    }
}

/// Desktop's wording for a change whose maker isn't known (`--unknown`).
fn unknown_text(change: &GroupNotice, name: &impl Fn(&str) -> String) -> Option<String> {
    use crate::i18n::{Arg, tf};
    let who = |m: &Option<String>| m.as_deref().map(name);
    Some(match change {
        GroupNotice::Title { title } if title.is_empty() => tr("A member removed the group name.").into(),
        GroupNotice::Title { title } => tf("icu:GroupV2--title--change--unknown", &format!("A member changed the group name to \"{title}\"."), &[("newTitle", Arg::from(title))]),
        GroupNotice::Description { description: Some(d) } if d.is_empty() => tr("The group description was removed.").into(),
        GroupNotice::Description { .. } => tr("The group description was changed.").into(),
        GroupNotice::Avatar => tr("A member changed the group avatar.").into(),
        GroupNotice::InfoAccess { admins_only: true } => tr("An admin changed who can edit group info to \"Only admins.\"").into(),
        GroupNotice::InfoAccess { admins_only: false } => tr("An admin changed who can edit group info to \"All members.\"").into(),
        GroupNotice::MemberAccess { admins_only: true } => tr("An admin changed who can edit group membership to \"Only admins.\"").into(),
        GroupNotice::MemberAccess { admins_only: false } => tr("An admin changed who can edit group membership to \"All members.\"").into(),
        GroupNotice::LabelAccess { admins_only: true } => tr("An admin changed who can add member labels to \"Only admins.\"").into(),
        GroupNotice::LabelAccess { admins_only: false } => tr("An admin changed who can add member labels to \"All members.\"").into(),
        GroupNotice::Added { member: None } => tr("You were added to the group.").into(),
        GroupNotice::Added { member } => {
            let m = who(member).unwrap_or_default();
            tf("icu:GroupV2--member-add--other--unknown", &format!("A member added {m}."), &[("memberName", Arg::from(&m))])
        }
        GroupNotice::Removed { member: None } => tr("You were removed from the group.").into(),
        GroupNotice::Removed { member } => {
            let m = who(member).unwrap_or_default();
            tf("icu:GroupV2--member-remove--other--unknown", &format!("A member removed {m}."), &[("memberName", Arg::from(&m))])
        }
        GroupNotice::Admin { member: None, admin: true } => tr("An admin made you an admin.").into(),
        GroupNotice::Admin { member: None, admin: false } => tr("An admin revoked your admin privileges.").into(),
        GroupNotice::Admin { member, admin } => {
            let m = who(member).unwrap_or_default();
            if *admin {
                tf("icu:GroupV2--member-privilege--promote--other--unknown", &format!("An admin made {m} an admin."), &[("memberName", Arg::from(&m))])
            } else {
                tf("icu:GroupV2--member-privilege--demote--other--unknown", &format!("An admin revoked admin privileges from {m}."), &[("memberName", Arg::from(&m))])
            }
        }
        GroupNotice::LinkReset => tr("The group link has been reset.").into(),
        GroupNotice::LinkOff => tr("The group link has been turned off.").into(),
        GroupNotice::AnnouncementsOnly { on: true } => tr("The group was changed to only allow admins to send messages.").into(),
        GroupNotice::AnnouncementsOnly { on: false } => tr("The group was changed to allow all members to send messages.").into(),
        _ => return None,
    })
}

fn group_text(by: Option<&str>, change: &GroupNotice, name: impl Fn(&str) -> String) -> String {
    if by == Some(langis_store::UNKNOWN_ACTOR)
        && let Some(text) = unknown_text(change, &name)
    {
        return text;
    }
    let who = by.map(&name);
    // "You …" or "Alice …".
    let actor = || who.clone().unwrap_or_else(|| tr("You").into());
    let admins_only = |only: bool| if only { "Only admins." } else { "All members." };
    match change {
        GroupNotice::Title { title } if title.is_empty() => crate::i18n::tf("icu:GroupV2--title--remove--other", &format!("{} removed the group name.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::Title { title } => format!("{} changed the group name to \"{title}\".", actor()),
        GroupNotice::Description { description: Some(d) } if d.is_empty() => match by {
            None => tr("You removed the group description.").into(),
            Some(_) => crate::i18n::tf("icu:GroupV2--description--remove--other", &format!("{} removed the group description.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
        },
        GroupNotice::Description { .. } => match by {
            None => tr("You changed the group description.").into(),
            Some(_) => crate::i18n::tf("icu:GroupV2--description--change--other", &format!("{} changed the group description.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
        },
        GroupNotice::Avatar => crate::i18n::tf("icu:GroupV2--avatar--change--other", &format!("{} changed the group avatar.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::Added { member: None } if by.is_none() => tr("You joined the group.").into(),
        GroupNotice::Added { member: None } => crate::i18n::tf("icu:GroupV2--member-add--you--other", &format!("{} added you to the group.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::Added { member: Some(m) } if by.is_none() => crate::i18n::tf("icu:GroupV2--member-add--other--you", &format!("You added {}.", name(m)), &[("memberName", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Added { member: Some(m) } if by == Some(m.as_str()) => crate::i18n::tf("icu:joinedTheGroup", &format!("{} joined the group.", name(m)), &[("name", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Added { member: Some(m) } => crate::i18n::tf("icu:GroupV2--member-add--other--other", &format!("{} added {}.", actor(), name(m)), &[("adderName", crate::i18n::Arg::from(&actor())), ("addeeName", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Removed { member: None } if by.is_none() => tr("You left the group.").into(),
        GroupNotice::Removed { member: None } => crate::i18n::tf("icu:GroupV2--member-remove--you--other", &format!("{} removed you.", actor()), &[("adminName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::Removed { member: Some(m) } if by == Some(m.as_str()) => crate::i18n::tf("icu:leftTheGroup", &format!("{} left the group.", name(m)), &[("name", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Removed { member: Some(m) } if by.is_none() => crate::i18n::tf("icu:GroupV2--member-remove--other--you", &format!("You removed {}.", name(m)), &[("memberName", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Removed { member: Some(m) } => crate::i18n::tf("icu:GroupV2--member-remove--other--other", &format!("{} removed {}.", actor(), name(m)), &[("adminName", crate::i18n::Arg::from(&actor())), ("memberName", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Admin { member: None, admin: true } => crate::i18n::tf("icu:GroupV2--member-privilege--promote--you--other", &format!("{} made you an admin.", actor()), &[("adminName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::Admin { member: None, admin: false } => crate::i18n::tf("icu:GroupV2--member-privilege--demote--you--other", &format!("{} revoked your admin privileges.", actor()), &[("adminName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::Admin { member: Some(m), admin: true } => crate::i18n::tf("icu:GroupV2--member-privilege--promote--other--other", &format!("{} made {} an admin.", actor(), name(m)), &[("adminName", crate::i18n::Arg::from(&actor())), ("memberName", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::Admin { member: Some(m), admin: false } => {
            crate::i18n::tf("icu:GroupV2--member-privilege--demote--other--other", &format!("{} revoked admin privileges from {}.", actor(), name(m)), &[("adminName", crate::i18n::Arg::from(&actor())), ("memberName", crate::i18n::Arg::from(&name(m)))])
        }
        GroupNotice::InfoAccess { admins_only: only } => {
            format!("{} changed who can edit group info to \"{}\"", actor(), admins_only(*only))
        }
        GroupNotice::MemberAccess { admins_only: only } => {
            format!("{} changed who can edit group membership to \"{}\"", actor(), admins_only(*only))
        }
        GroupNotice::LabelAccess { admins_only: only } => {
            format!("{} changed who can add member labels to \"{}\"", actor(), admins_only(*only))
        }
        GroupNotice::AnnouncementsOnly { on: true } => {
            crate::i18n::tf("icu:GroupV2--announcements--admin--other", &format!("{} changed the group settings to only allow admins to send messages.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))])
        }
        GroupNotice::AnnouncementsOnly { on: false } => {
            crate::i18n::tf("icu:GroupV2--announcements--member--other", &format!("{} changed the group settings to allow all members to send messages.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))])
        }
        // Desktop's `GroupV2--terminated--you` and `--other`, without a period.
        GroupNotice::Ended => crate::i18n::tf("icu:GroupV2--terminated--other", &format!("{} ended the group", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
        _ => membership_text(by, change, name),
    }
}

/// "1 person" or "3 people".
fn people(count: u32) -> String {
    if count == 1 { "1 person".into() } else { crate::i18n::tf("icu:CallControls__InfoDisplay--participants", &format!("{count} people"), &[("count", crate::i18n::Arg::from(&count))]) }
}

/// "an invitation to the group for 1 person" or "invitations to the
/// group for 3 people".
fn invitations(count: u32) -> String {
    if count == 1 { "an invitation to the group for 1 person".into() } else { format!("invitations to the group for {count} people") }
}

/// Invitations, requests to join and the group link, in Desktop's words
/// (`renderChangeDetail` in `ts/groupChange.std.ts`, `icu:GroupV2--…`).
fn membership_text(by: Option<&str>, change: &GroupNotice, name: impl Fn(&str) -> String) -> String {
    use langis_store::Inviter;
    let who = by.map(&name);
    let actor = || who.clone().unwrap_or_else(|| tr("You").into());
    let same = |member: &Option<String>| member.as_deref() == by;
    match change {
        GroupNotice::JoinedByLink { member: None } => tr("You joined the group via the group link.").into(),
        GroupNotice::JoinedByLink { member: Some(m) } => crate::i18n::tf("icu:GroupV2--member-add-from-link--other", &format!("{} joined the group via the group link.", name(m)), &[("memberName", crate::i18n::Arg::from(&name(m)))]),
        GroupNotice::AcceptedInvite { member: None, inviter: Inviter::Person { id } } => {
            crate::i18n::tf("icu:GroupV2--member-add--from-invite--you", &format!("You accepted an invitation to the group from {}.", name(id)), &[("inviterName", crate::i18n::Arg::from(&name(id)))])
        }
        GroupNotice::AcceptedInvite { member: None, .. } => tr("You accepted an invitation to the group.").into(),
        GroupNotice::AcceptedInvite { member: Some(m), inviter: Inviter::Us } => {
            crate::i18n::tf("icu:GroupV2--member-add--from-invite--from-you", &format!("{} accepted your invitation to the group.", name(m)), &[("inviteeName", crate::i18n::Arg::from(&name(m)))])
        }
        GroupNotice::AcceptedInvite { member: Some(m), inviter: Inviter::Person { id } } => {
            crate::i18n::tf("icu:GroupV2--member-add--from-invite--other", &format!("{} accepted an invitation to the group from {}.", name(m), name(id)), &[("inviteeName", crate::i18n::Arg::from(&name(m))), ("inviterName", crate::i18n::Arg::from(&name(id)))])
        }
        GroupNotice::AcceptedInvite { member: Some(m), inviter: Inviter::Unknown } => {
            crate::i18n::tf("icu:GroupV2--member-add--from-invite--other-no-from", &format!("{} accepted an invitation to the group.", name(m)), &[("inviteeName", crate::i18n::Arg::from(&name(m)))])
        }
        GroupNotice::Approved { member: None } => match &who {
            Some(admin) => crate::i18n::tf("icu:GroupV2--member-add-from-admin-approval--you--other", &format!("{admin} approved your request to join the group."), &[("adminName", crate::i18n::Arg::from(&admin))]),
            None => tr("Your request to join the group has been approved.").into(),
        },
        GroupNotice::Approved { member: Some(m) } => match &who {
            Some(admin) => crate::i18n::tf("icu:GroupV2--member-add-from-admin-approval--other--other", &format!("{admin} approved a request to join the group from {}.", name(m)), &[("adminName", crate::i18n::Arg::from(&admin)), ("joinerName", crate::i18n::Arg::from(&name(m)))]),
            None => crate::i18n::tf("icu:GroupV2--member-add-from-admin-approval--other--you", &format!("You approved a request to join the group from {}.", name(m)), &[("joinerName", crate::i18n::Arg::from(&name(m)))]),
        },
        GroupNotice::Invited { member: None } => match &who {
            Some(admin) => crate::i18n::tf("icu:GroupV2--pending-add--one--you--other", &format!("{admin} invited you to the group."), &[("memberName", crate::i18n::Arg::from(&admin))]),
            None => tr("You were invited to the group.").into(),
        },
        GroupNotice::Invited { member: Some(m) } => match &who {
            Some(member) => crate::i18n::tf("icu:GroupV2--pending-add--one--other--other", &format!("{member} invited 1 person to the group."), &[("memberName", crate::i18n::Arg::from(&member))]),
            None => crate::i18n::tf("icu:GroupV2--pending-add--one--other--you", &format!("You invited {} to the group.", name(m)), &[("inviteeName", crate::i18n::Arg::from(&name(m)))]),
        },
        GroupNotice::InvitedMany { count } => format!("{} invited {} to the group.", actor(), people(*count)),
        GroupNotice::InviteRemoved { member, inviter } => {
            let invitee = || member.as_deref().map_or_else(|| "you".to_owned(), &name);
            match (inviter, same(member)) {
                (Inviter::Us, true) => crate::i18n::tf("icu:GroupV2--pending-remove--decline--you", &format!("{} declined your invitation to the group.", invitee()), &[("inviteeName", crate::i18n::Arg::from(&invitee()))]),
                (Inviter::Us, false) => match &who {
                    Some(admin) => crate::i18n::tf("icu:GroupV2--pending-remove--revoke-invite-from-you--one--other", &format!("{admin} revoked the invitation to the group you sent to {}.", invitee()), &[("adminName", crate::i18n::Arg::from(&admin)), ("inviteeName", crate::i18n::Arg::from(&invitee()))]),
                    None => crate::i18n::tf("icu:GroupV2--pending-remove--revoke-invite-from-you--one--you", &format!("You rescinded your invitation to {}.", invitee()), &[("inviteeName", crate::i18n::Arg::from(&invitee()))]),
                },
                (_, true) if by.is_none() => tr("You declined the invitation to the group.").into(),
                (Inviter::Person { id }, true) => crate::i18n::tf("icu:GroupV2--pending-remove--decline--other", &format!("1 person invited by {} declined the invitation to the group.", name(id)), &[("memberName", crate::i18n::Arg::from(&name(id)))]),
                (_, true) => tr("1 person declined their invitation to the group.").into(),
                (Inviter::Person { id }, false) if by == Some(id.as_str()) => match member {
                    None => crate::i18n::tf("icu:GroupV2--pending-remove--revoke-own--to-you", &format!("{} revoked their invitation to you.", name(id)), &[("inviterName", crate::i18n::Arg::from(&name(id)))]),
                    Some(_) => crate::i18n::tf("icu:GroupV2--pending-remove--revoke-own--unknown", &format!("{} revoked their invitation to 1 person.", name(id)), &[("inviterName", crate::i18n::Arg::from(&name(id)))]),
                },
                (Inviter::Person { id }, false) => {
                    crate::i18n::tf("icu:GroupV2--pending-remove--revoke-invite-from--one--other", &format!("{} revoked an invitation to the group for 1 person invited by {}.", actor(), name(id)), &[("adminName", crate::i18n::Arg::from(&actor())), ("memberName", crate::i18n::Arg::from(&name(id)))])
                }
                (Inviter::Unknown, false) => crate::i18n::tf("icu:GroupV2--pending-remove--revoke--one--other", &format!("{} revoked an invitation to the group for 1 person.", actor()), &[("memberName", crate::i18n::Arg::from(&actor()))]),
            }
        }
        GroupNotice::InvitesRemoved { count, inviter } => match inviter {
            Inviter::Us => match &who {
                Some(admin) => format!(
                    "{admin} revoked the {}.",
                    if *count == 1 {
                        "invitation to the group you sent to 1 person".to_owned()
                    } else {
                        format!("invitations to the group you sent to {count} people")
                    }
                ),
                None => crate::i18n::tf("icu:GroupV2--pending-remove--revoke-invite-from-you--one--you", &format!("You rescinded your invitation to {}.", people(*count)), &[("inviteeName", crate::i18n::Arg::from(&people(*count)))]),
            },
            Inviter::Person { id } => format!("{} revoked {} invited by {}.", actor(), invitations(*count), name(id)),
            Inviter::Unknown => format!("{} revoked {}.", actor(), invitations(*count)),
        },
        GroupNotice::Requested { member: None } => tr("You sent a request to join the group.").into(),
        GroupNotice::Requested { member: Some(m) } => crate::i18n::tf("icu:GroupV2--admin-approval-add-one--other", &format!("{} requested to join via the group link.", name(m)), &[("joinerName", crate::i18n::Arg::from(&name(m)))]),
        // Desktop's `admin-approval-bounce`: a second line while they're
        // asking again.
        GroupNotice::RequestBounce { member, times, pending } => {
            let n = name(member);
            let requests = if *times == 1 { "their request".to_owned() } else { format!("{times} requests") };
            let text = crate::i18n::tf("icu:GroupV2--admin-approval-bounce--pluralized", &format!("{n} requested and canceled {requests} to join via the group link"), &[("joinerName", crate::i18n::Arg::from(&n)), ("numberOfRequests", crate::i18n::Arg::from(*times))]);
            if !*pending {
                return text;
            }
            let again = crate::i18n::tf("icu:GroupV2--admin-approval-add-one--other", &format!("{n} requested to join via the group link."), &[("joinerName", crate::i18n::Arg::from(&n))]);
            format!("{text}\n{again}")
        }
        GroupNotice::RequestRemoved { member: None } if by.is_none() => tr("You canceled your request to join the group.").into(),
        GroupNotice::RequestRemoved { member: None } => tr("Your request to join the group has been denied by an admin.").into(),
        GroupNotice::RequestRemoved { member: Some(m) } if by == Some(m.as_str()) => {
            crate::i18n::tf("icu:GroupV2--admin-approval-remove-one--other--own", &format!("{} canceled their request to join the group.", name(m)), &[("joinerName", crate::i18n::Arg::from(&name(m)))])
        }
        GroupNotice::RequestRemoved { member: Some(m) } => match &who {
            Some(admin) => crate::i18n::tf("icu:GroupV2--admin-approval-remove-one--other--other", &format!("{admin} denied a request to join the group from {}.", name(m)), &[("adminName", crate::i18n::Arg::from(&admin)), ("joinerName", crate::i18n::Arg::from(&name(m)))]),
            None => crate::i18n::tf("icu:GroupV2--admin-approval-remove-one--other--you", &format!("You denied a request to join the group from {}.", name(m)), &[("joinerName", crate::i18n::Arg::from(&name(m)))]),
        },
        GroupNotice::LinkOn { approval } => format!(
            "{} turned on the group link with admin approval {}.",
            actor(),
            if *approval { "enabled" } else { "disabled" }
        ),
        GroupNotice::LinkOff => crate::i18n::tf("icu:GroupV2--group-link-remove--other", &format!("{} turned off the group link.", actor()), &[("adminName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::LinkReset => crate::i18n::tf("icu:GroupV2--group-link-reset--other", &format!("{} reset the group link.", actor()), &[("adminName", crate::i18n::Arg::from(&actor()))]),
        GroupNotice::LinkApproval { on } => {
            format!("{} {} admin approval for the group link.", actor(), if *on { "enabled" } else { "disabled" })
        }
        // The rest are `group_text`'s.
        _ => String::new(),
    }
}


/// Desktop's `SystemMessage` icons (`SystemMessage--icon-*`), 16px before
/// a notice's text: their SVGs, by name.
pub fn icon_source(name: &str) -> Option<(&'static str, &'static [u8])> {
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            match name {
                $($name => Some((concat!("bytes://notice/", $name, ".svg"), include_bytes!(concat!("../assets/icons/notice/", $name, ".svg")).as_slice())),)*
                _ => None,
            }
        };
    }
    icons!(
        "phone-compact", "group-compact", "megaphone-compact", "person-plus-compact", "person-check-compact", "photo-square-compact",
        "person-x-compact", "edit-compact", "leave-compact", "person-minus-compact", "info-compact", "group-x-inline", "person-compact",
        "safety_number-compact", "refresh", "thread-compact", "timer-compact", "timer-slash-compact", "error-circle-compact", "video-compact",
        "merge-compact", "spam", "block",
    )
}

/// The icon a notice gets (Desktop's per-notification `icon=`), and
/// whether it's drawn in the destructive color (`SystemMessage--danger`:
/// missed and declined calls).
pub fn notice_icon(notice: &Notice) -> (Option<&'static str>, bool) {
    use langis_store::{CallOutcome, GroupCallStatus, MessageRequestEvent};
    let icon = match notice {
        Notice::Timer { seconds: 0, .. } => "timer-slash-compact",
        Notice::Timer { .. } => "timer-compact",
        Notice::Pinned { .. } | Notice::PollEnded { .. } => return (None, false),
        Notice::Group { by, change } => match change {
            GroupNotice::Title { .. } | GroupNotice::Description { .. } => "edit-compact",
            GroupNotice::Avatar => "photo-square-compact",
            GroupNotice::Added { .. } | GroupNotice::JoinedByLink { .. } | GroupNotice::AcceptedInvite { .. } | GroupNotice::Invited { .. } | GroupNotice::InvitedMany { .. } => "person-plus-compact",
            GroupNotice::Removed { member } if member.as_deref() == by.as_deref() || (member.is_none() && by.is_none()) => "leave-compact",
            GroupNotice::Removed { .. } => "person-minus-compact",
            GroupNotice::Approved { .. } => "person-check-compact",
            // Desktop's `group-decline`, its default when one's still asking.
            GroupNotice::RequestBounce { pending: false, .. } | GroupNotice::RequestRemoved { .. } | GroupNotice::InviteRemoved { .. } | GroupNotice::InvitesRemoved { .. } => "person-x-compact",
            GroupNotice::InfoAccess { .. } | GroupNotice::MemberAccess { .. } | GroupNotice::LabelAccess { .. } | GroupNotice::AnnouncementsOnly { .. } => "megaphone-compact",
            GroupNotice::LinkOn { .. } | GroupNotice::LinkOff | GroupNotice::LinkReset | GroupNotice::LinkApproval { .. } => "megaphone-compact",
            GroupNotice::Ended => "group-x-inline",
            _ => "group-compact",
        },
        Notice::SafetyNumberChanged { .. } | Notice::Verified { .. } => "safety_number-compact",
        Notice::Call { incoming, video, outcome, .. } => {
            let danger = *incoming && matches!(outcome, CallOutcome::Missed | CallOutcome::Declined);
            return (Some(if *video { "video-compact" } else { "phone-compact" }), danger);
        }
        notice @ Notice::GroupCall { status, .. } => {
            let danger = notice.group_call_incoming() && matches!(status, Some(GroupCallStatus::Missed | GroupCallStatus::Declined));
            return (Some("video-compact"), danger);
        }
        Notice::MessageRequest { event, .. } => match event {
            MessageRequestEvent::Accept => "thread-compact",
            MessageRequestEvent::Block | MessageRequestEvent::Unblock => "block",
            MessageRequestEvent::Spam => "spam",
        },
        Notice::ProfileNameChanged { .. } => "person-compact",
        Notice::NumberChanged { .. } => "phone-compact",
        Notice::SessionRefreshed | Notice::SessionEnded => "refresh",
        Notice::JoinedSignal { .. } | Notice::NumberIsTheirs { .. } | Notice::DeliveryIssue { .. } | Notice::ContactRemoved => "info-compact",
        Notice::Merged { .. } => "merge-compact",
        Notice::StartedWith { .. } => "thread-compact",
        Notice::UnsupportedMessage { .. } => "error-circle-compact",
    };
    (Some(icon), false)
}

/// Notices Desktop follows with " · {time}" (calls, and a contact joining).
pub fn shows_time(notice: &Notice) -> bool {
    matches!(notice, Notice::Call { .. } | Notice::GroupCall { .. } | Notice::JoinedSignal { .. } | Notice::NumberChanged { .. })
}

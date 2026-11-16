//! What timeline notices say, in Signal Desktop's words. Ported from
//! Vernam's `views/notices.rs` (AGPL-3.0, Ken Powers) without its UI parts.

use langis_store::{GroupNotice, Notice};

use crate::format::timer_long;

/// `name(sid)` gives a person's display name (`sid` is never `None`).
pub fn notice_text(notice: &Notice, name: impl Fn(&str) -> String) -> String {
    match notice {
        Notice::Timer { by: None, seconds: 0 } => "You disabled disappearing messages.".into(),
        Notice::Timer { by: Some(by), seconds: 0 } => format!("{} disabled disappearing messages.", name(by)),
        Notice::Timer { by: None, seconds } => {
            format!("You set the disappearing message time to {}.", timer_long(*seconds))
        }
        Notice::Timer { by: Some(by), seconds } => {
            format!("{} set the disappearing message time to {}.", name(by), timer_long(*seconds))
        }
        Notice::Pinned { by: None, .. } => "You pinned a message".into(),
        Notice::Pinned { by: Some(by), .. } => format!("{} pinned a message", name(by)),
        Notice::Group { by, change } => group_text(by.as_deref(), change, name),
        Notice::SafetyNumberChanged { who } => format!("Your safety number with {} has changed.", name(who)),
        Notice::Call { incoming, video, outcome, .. } => call_text(*incoming, *video, *outcome),
        notice @ Notice::GroupCall { by, status, .. } => group_call_text(notice.group_call_incoming(), by.as_deref(), *status, name),
        // Desktop's `icu:PollTerminate--you` / `--other`.
        Notice::PollEnded { by: None, question, .. } => format!("You ended the poll: {question}"),
        Notice::PollEnded { by: Some(by), question, .. } => format!("{} ended the poll: {question}", name(by)),
        Notice::MessageRequest { event, group, who } => message_request_text(*event, *group, who.as_deref(), name),
        // Without their address book name; see `profile_change_text`.
        Notice::ProfileNameChanged { old_name, new_name, .. } => profile_change_text(old_name, new_name, None),
        // Desktop's `icu:ChangeNumber--notification`.
        Notice::NumberChanged { who } => format!("{} changed their phone number", name(who)),
        // `icu:ChatRefresh--notification`.
        Notice::SessionRefreshed => "Chat session refreshed".into(),
        // `icu:DeliveryIssue--notification`.
        Notice::DeliveryIssue { who } => format!("A message from {} couldn\u{2019}t be delivered", name(who)),
        // `icu:Message--unsupported-message` and `--from-me-unsupported-message`.
        Notice::UnsupportedMessage { who: Some(who) } => format!(
            "{} sent you a message that can't be processed or displayed because it uses a new Signal feature.",
            name(who)
        ),
        Notice::UnsupportedMessage { who: None } => {
            "One of your devices sent a message that can't be processed or displayed because it uses a new Signal feature.".into()
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
        Some(sender) => format!("{sender} changed their profile name from {old_name} to {new_name}."),
        None => format!("{old_name} changed their profile name to {new_name}."),
    }
}

/// The icon Desktop's `SystemMessage` shows beside a notice, for the
/// notices that have one here.
fn message_request_text(event: langis_store::MessageRequestEvent, group: bool, who: Option<&str>, name: impl Fn(&str) -> String) -> String {
    use langis_store::MessageRequestEvent;
    match (event, group, who) {
        (MessageRequestEvent::Accept, false, Some(who)) => format!("You accepted {}'s message request", name(who)),
        (MessageRequestEvent::Accept, _, _) => "You accepted the message request".into(),
        (MessageRequestEvent::Block, true, _) => "You blocked the group".into(),
        (MessageRequestEvent::Block, false, _) => "You blocked this person".into(),
        (MessageRequestEvent::Unblock, true, _) => "You unblocked the group".into(),
        (MessageRequestEvent::Unblock, false, _) => "You unblocked this person".into(),
        (MessageRequestEvent::Spam, _, _) => "Reported as spam".into(),
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

fn group_text(by: Option<&str>, change: &GroupNotice, name: impl Fn(&str) -> String) -> String {
    let who = by.map(&name);
    // "You …" or "Alice …".
    let actor = || who.clone().unwrap_or_else(|| "You".into());
    let admins_only = |only: bool| if only { "Only admins." } else { "All members." };
    match change {
        GroupNotice::Title { title } if title.is_empty() => format!("{} removed the group name.", actor()),
        GroupNotice::Title { title } => format!("{} changed the group name to \"{title}\".", actor()),
        GroupNotice::Description => format!("{} changed the group description.", actor()),
        GroupNotice::Avatar => format!("{} changed the group avatar.", actor()),
        GroupNotice::Added { member: None } if by.is_none() => "You joined the group.".into(),
        GroupNotice::Added { member: None } => format!("{} added you to the group.", actor()),
        GroupNotice::Added { member: Some(m) } if by.is_none() => format!("You added {}.", name(m)),
        GroupNotice::Added { member: Some(m) } if by == Some(m.as_str()) => format!("{} joined the group.", name(m)),
        GroupNotice::Added { member: Some(m) } => format!("{} added {}.", actor(), name(m)),
        GroupNotice::Removed { member: None } if by.is_none() => "You left the group.".into(),
        GroupNotice::Removed { member: None } => format!("{} removed you.", actor()),
        GroupNotice::Removed { member: Some(m) } if by == Some(m.as_str()) => format!("{} left the group.", name(m)),
        GroupNotice::Removed { member: Some(m) } if by.is_none() => format!("You removed {}.", name(m)),
        GroupNotice::Removed { member: Some(m) } => format!("{} removed {}.", actor(), name(m)),
        GroupNotice::Admin { member: None, admin: true } => format!("{} made you an admin.", actor()),
        GroupNotice::Admin { member: None, admin: false } => format!("{} revoked your admin privileges.", actor()),
        GroupNotice::Admin { member: Some(m), admin: true } => format!("{} made {} an admin.", actor(), name(m)),
        GroupNotice::Admin { member: Some(m), admin: false } => {
            format!("{} revoked admin privileges from {}.", actor(), name(m))
        }
        GroupNotice::InfoAccess { admins_only: only } => {
            format!("{} changed who can edit group info to \"{}\"", actor(), admins_only(*only))
        }
        GroupNotice::MemberAccess { admins_only: only } => {
            format!("{} changed who can edit group membership to \"{}\"", actor(), admins_only(*only))
        }
        GroupNotice::AnnouncementsOnly { on: true } => {
            format!("{} changed the group settings to only allow admins to send messages.", actor())
        }
        GroupNotice::AnnouncementsOnly { on: false } => {
            format!("{} changed the group settings to allow all members to send messages.", actor())
        }
        // Desktop's `GroupV2--terminated--you` and `--other`, without a period.
        GroupNotice::Ended => format!("{} ended the group", actor()),
        _ => membership_text(by, change, name),
    }
}

/// "1 person" or "3 people".
fn people(count: u32) -> String {
    if count == 1 { "1 person".into() } else { format!("{count} people") }
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
    let actor = || who.clone().unwrap_or_else(|| "You".into());
    let same = |member: &Option<String>| member.as_deref() == by;
    match change {
        GroupNotice::JoinedByLink { member: None } => "You joined the group via the group link.".into(),
        GroupNotice::JoinedByLink { member: Some(m) } => format!("{} joined the group via the group link.", name(m)),
        GroupNotice::AcceptedInvite { member: None, inviter: Inviter::Person { id } } => {
            format!("You accepted an invitation to the group from {}.", name(id))
        }
        GroupNotice::AcceptedInvite { member: None, .. } => "You accepted an invitation to the group.".into(),
        GroupNotice::AcceptedInvite { member: Some(m), inviter: Inviter::Us } => {
            format!("{} accepted your invitation to the group.", name(m))
        }
        GroupNotice::AcceptedInvite { member: Some(m), inviter: Inviter::Person { id } } => {
            format!("{} accepted an invitation to the group from {}.", name(m), name(id))
        }
        GroupNotice::AcceptedInvite { member: Some(m), inviter: Inviter::Unknown } => {
            format!("{} accepted an invitation to the group.", name(m))
        }
        GroupNotice::Approved { member: None } => match &who {
            Some(admin) => format!("{admin} approved your request to join the group."),
            None => "Your request to join the group has been approved.".into(),
        },
        GroupNotice::Approved { member: Some(m) } => match &who {
            Some(admin) => format!("{admin} approved a request to join the group from {}.", name(m)),
            None => format!("You approved a request to join the group from {}.", name(m)),
        },
        GroupNotice::Invited { member: None } => match &who {
            Some(admin) => format!("{admin} invited you to the group."),
            None => "You were invited to the group.".into(),
        },
        GroupNotice::Invited { member: Some(m) } => match &who {
            Some(member) => format!("{member} invited 1 person to the group."),
            None => format!("You invited {} to the group.", name(m)),
        },
        GroupNotice::InvitedMany { count } => format!("{} invited {} to the group.", actor(), people(*count)),
        GroupNotice::InviteRemoved { member, inviter } => {
            let invitee = || member.as_deref().map_or_else(|| "you".to_owned(), &name);
            match (inviter, same(member)) {
                (Inviter::Us, true) => format!("{} declined your invitation to the group.", invitee()),
                (Inviter::Us, false) => match &who {
                    Some(admin) => format!("{admin} revoked the invitation to the group you sent to {}.", invitee()),
                    None => format!("You rescinded your invitation to {}.", invitee()),
                },
                (_, true) if by.is_none() => "You declined the invitation to the group.".into(),
                (Inviter::Person { id }, true) => format!("1 person invited by {} declined the invitation to the group.", name(id)),
                (_, true) => "1 person declined their invitation to the group.".into(),
                (Inviter::Person { id }, false) if by == Some(id.as_str()) => match member {
                    None => format!("{} revoked their invitation to you.", name(id)),
                    Some(_) => format!("{} revoked their invitation to 1 person.", name(id)),
                },
                (Inviter::Person { id }, false) => {
                    format!("{} revoked an invitation to the group for 1 person invited by {}.", actor(), name(id))
                }
                (Inviter::Unknown, false) => format!("{} revoked an invitation to the group for 1 person.", actor()),
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
                None => format!("You rescinded your invitation to {}.", people(*count)),
            },
            Inviter::Person { id } => format!("{} revoked {} invited by {}.", actor(), invitations(*count), name(id)),
            Inviter::Unknown => format!("{} revoked {}.", actor(), invitations(*count)),
        },
        GroupNotice::Requested { member: None } => "You sent a request to join the group.".into(),
        GroupNotice::Requested { member: Some(m) } => format!("{} requested to join via the group link.", name(m)),
        GroupNotice::RequestRemoved { member: None } if by.is_none() => "You canceled your request to join the group.".into(),
        GroupNotice::RequestRemoved { member: None } => "Your request to join the group has been denied by an admin.".into(),
        GroupNotice::RequestRemoved { member: Some(m) } if by == Some(m.as_str()) => {
            format!("{} canceled their request to join the group.", name(m))
        }
        GroupNotice::RequestRemoved { member: Some(m) } => match &who {
            Some(admin) => format!("{admin} denied a request to join the group from {}.", name(m)),
            None => format!("You denied a request to join the group from {}.", name(m)),
        },
        GroupNotice::LinkOn { approval } => format!(
            "{} turned on the group link with admin approval {}.",
            actor(),
            if *approval { "enabled" } else { "disabled" }
        ),
        GroupNotice::LinkOff => format!("{} turned off the group link.", actor()),
        GroupNotice::LinkReset => format!("{} reset the group link.", actor()),
        GroupNotice::LinkApproval { on } => {
            format!("{} {} admin approval for the group link.", actor(), if *on { "enabled" } else { "disabled" })
        }
        // The rest are `group_text`'s.
        _ => String::new(),
    }
}


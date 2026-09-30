//! TypeScript membership unions. Plan Decision 6 allows no compatibility
//! names, so `createGroup`, `createDm`, `addMembers`, and `removeMembers` take
//! inbox IDs or account identities, and the identity routes stay private.

use anyhow::{Result, bail};

/// One public union and the two binding methods it routes to.
pub(crate) struct Route {
    pub(crate) owner: &'static str,
    pub(crate) method: &'static str,
    pub(crate) identity: &'static str,
    /// Argument names, in order, after the member argument.
    rest: &'static [&'static str],
    /// The member argument name.
    pub(crate) member: &'static str,
    pub(crate) list: bool,
}

pub(crate) const ROUTES: &[Route] = &[
    Route {
        owner: "Conversations",
        method: "createGroup",
        identity: "createGroupWithIdentities",
        member: "members",
        rest: &["options", "asyncOpts_"],
        list: true,
    },
    Route {
        owner: "Conversations",
        method: "createDm",
        identity: "createDmWithIdentity",
        member: "peer",
        rest: &["options", "asyncOpts_"],
        list: false,
    },
    Route {
        owner: "Group",
        method: "addMembers",
        identity: "addMembersByIdentity",
        member: "members",
        rest: &["asyncOpts_"],
        list: true,
    },
    Route {
        owner: "Group",
        method: "removeMembers",
        identity: "removeMembersByIdentity",
        member: "members",
        rest: &["asyncOpts_"],
        list: true,
    },
];

/// Type guards that route a member argument without a cast. An empty list uses
/// inbox IDs. A list that mixes inbox IDs and account identities fails before
/// any call.
pub(crate) fn helper(error: &str) -> String {
    format!(
        "\n/** True for an account identity list. A mixed list fails before any call. */\nfunction identityMembers<I, P>(value: Array<I> | Array<P>): value is Array<P> {{\n  const items: ReadonlyArray<unknown> = value;\n  const identities = items.filter((item) => typeof item !== \"string\").length;\n  if (identities !== 0 && identities !== items.length)\n    throw {error}.InvalidArgument.new({{ code: \"InvalidArgument\", category: {category}.Input, retryable: false, message: \"a member list mixes inbox IDs and account identities\" }});\n  return identities !== 0;\n}}\n/** True for an account identity. */\nfunction identityMember<I, P>(value: I | P): value is P {{\n  return typeof value !== \"string\";\n}}\n",
        category = error.replace("XmtpError", "ErrorCategory"),
    )
}

/// The body of a union method that routes to its two binding methods.
pub(crate) fn union_body(route: &Route, inbox: &str, identity: &str) -> String {
    let rest = route
        .rest
        .iter()
        .map(|name| format!(", {name}"))
        .collect::<String>();
    let guard = if route.list {
        "identityMembers"
    } else {
        "identityMember"
    };
    format!(
        "return {guard}<{inbox}, {identity}>({member}) ? this.{id_method}({member}{rest}) : this.{method}ByInboxIds({member}{rest});",
        member = route.member,
        id_method = route.identity,
        method = route.method,
    )
}

fn member_type(route: &Route, inbox: &str) -> String {
    if route.list {
        format!("{}: Array<{inbox}>", route.member)
    } else {
        format!("{}: {inbox}", route.member)
    }
}

fn union_type(route: &Route, inbox: &str, identity: &str) -> String {
    if route.list {
        format!("{}: Array<{inbox}> | Array<{identity}>", route.member)
    } else {
        format!("{}: {inbox} | {identity}", route.member)
    }
}

/// Rewrite the generated binding: each interface declares the union and
/// omits the identity route, and each class adds the union method and makes
/// both routes private.
pub(crate) fn rewrite(source: &str) -> Result<String> {
    let mut output = source.to_owned();
    for route in ROUTES {
        let inbox_type = member_type(route, "InboxId");
        let union = union_type(route, "InboxId", "PublicIdentity");
        // Interface: the union replaces the inbox form; the identity form goes.
        let declaration = format!("    {}({inbox_type}, ", route.method);
        let identity_declaration = format!("    {}(", route.identity);
        let lines = output.lines().collect::<Vec<_>>();
        let declared = |prefix: &str| {
            lines
                .iter()
                .filter(|line| line.starts_with(prefix) && line.ends_with(';'))
                .count()
        };
        if declared(&declaration) != 1 || declared(&identity_declaration) != 1 {
            bail!(
                "generated TypeScript {}.{} changed shape",
                route.owner,
                route.method
            );
        }
        let mut kept: Vec<String> = Vec::with_capacity(lines.len());
        for line in lines {
            if line.starts_with(&identity_declaration) && line.ends_with(';') {
                // Drop the doc comment that belongs to the removed declaration.
                if kept.last().is_some_and(|last| last.trim() == "*/") {
                    while let Some(last) = kept.pop() {
                        if last.trim_start().starts_with("/**") {
                            break;
                        }
                    }
                }
                continue;
            }
            if line.starts_with(&declaration) && line.ends_with(';') {
                kept.push(line.replacen(&inbox_type, &union, 1));
            } else {
                kept.push(line.to_owned());
            }
        }
        output = kept.join("\n") + "\n";
        // Class: add the union and make both binding routes private.
        let method = format!("    async {}({inbox_type}, ", route.method);
        let identity_method = format!("    async {}(", route.identity);
        if output.matches(&method).count() != 1 || output.matches(&identity_method).count() != 1 {
            bail!("generated TypeScript {} method changed shape", route.method);
        }
        let start = output.find(&method).expect("checked method");
        let end = start + output[start..].find('\n').expect("method line");
        let signature = &output[start..end];
        let union_method = format!(
            "{}\n        {}\n    }}\n    private async {}ByInboxIds({inbox_type}, ",
            signature.replacen(&inbox_type, &union, 1),
            union_body(route, "InboxId", "PublicIdentity"),
            route.method,
        );
        output = format!(
            "{}{union_method}{}",
            &output[..start],
            &output[start + method.len()..]
        )
        .replacen(
            &identity_method,
            &format!("    private async {}(", route.identity),
            1,
        );
    }
    output.push_str(&helper("XmtpError"));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> String {
        let mut source = String::new();
        for route in ROUTES {
            let (params, rest) = if route.rest.len() == 2 {
                (
                    "options: X | undefined, asyncOpts_?: S",
                    "options: X | undefined = undefined, asyncOpts_?: S",
                )
            } else {
                ("asyncOpts_?: S", "asyncOpts_?: S")
            };
            let inbox = member_type(route, "InboxId");
            let identity = if route.list {
                "Array<PublicIdentity>"
            } else {
                "PublicIdentity"
            };
            source.push_str(&format!(
                "    {m}({inbox}, {params}) /*throws*/: Promise<R>;\n/**\n * Identity form.\n */\n    {i}({mem}: {identity}, {params}) /*throws*/: Promise<R>;\n    async {m}({inbox}, {rest}): Promise<R> /*throws*/ {{\n    }}\n    async {i}({mem}: {identity}, {rest}): Promise<R> /*throws*/ {{\n    }}\n",
                m = route.method,
                i = route.identity,
                mem = route.member,
            ));
        }
        source
    }

    // Each union routes to both methods; the identity routes leave the public API.
    #[xmtp_common::test(unwrap_try = true)]
    fn membership_unions_replace_identity_routes() {
        let output = rewrite(&binding())?;
        assert!(output.contains(
            "    createGroup(members: Array<InboxId> | Array<PublicIdentity>, options: X | undefined, asyncOpts_?: S) /*throws*/: Promise<R>;"
        ));
        assert!(output.contains("    createDm(peer: InboxId | PublicIdentity, "));
        assert!(output.contains(
            "return identityMembers<InboxId, PublicIdentity>(members) ? this.createGroupWithIdentities(members, options, asyncOpts_) : this.createGroupByInboxIds(members, options, asyncOpts_);"
        ));
        assert!(output.contains(
            "return identityMember<InboxId, PublicIdentity>(peer) ? this.createDmWithIdentity(peer, options, asyncOpts_) : this.createDmByInboxIds(peer, options, asyncOpts_);"
        ));
        for route in ROUTES {
            assert!(!output.contains(&format!("\n    {}(", route.identity)));
            assert!(output.contains(&format!("    private async {}(", route.identity)));
            assert!(output.contains(&format!("    private async {}ByInboxIds(", route.method)));
        }
        assert!(!output.contains("Identity form."));
        assert!(output.contains(
            "function identityMembers<I, P>(value: Array<I> | Array<P>): value is Array<P>"
        ));
        assert!(rewrite("changed").is_err());
    }
}

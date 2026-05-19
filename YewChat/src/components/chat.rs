use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_agent::{Bridge, Bridged};

use crate::services::event_bus::EventBus;
use crate::{services::websocket::WebsocketService, User};

pub enum Msg {
    HandleMsg(String),
    SubmitMessage,
    SelectThread(u64),
    AddReaction(u64, String),
    SubmitThreadReply,
}

#[derive(Deserialize)]
struct MessageData {
    id: u64,
    from: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReactionData {
    message_id: u64,
    emoji: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadData {
    message_id: u64,
    from: String,
    message: String,
}

#[derive(Clone)]
struct ChatMessage {
    id: u64,
    from: String,
    message: String,
}

#[derive(Clone)]
struct ThreadReply {
    author: String,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MsgTypes {
    Users,
    Register,
    Message,
    Reaction,
    Thread,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebSocketMessage {
    message_type: MsgTypes,
    data_array: Option<Vec<String>>,
    data: Option<String>,
}

#[derive(Clone)]
struct UserProfile {
    name: String,
    avatar: String,
}

pub struct Chat {
    users: Vec<UserProfile>,
    chat_input: NodeRef,
    thread_input: NodeRef,
    _producer: Box<dyn Bridge<EventBus>>,
    wss: WebsocketService,
    messages: Vec<ChatMessage>,
    reactions: BTreeMap<u64, BTreeMap<String, u32>>,
    threads: BTreeMap<u64, Vec<ThreadReply>>,
    selected_thread: Option<u64>,
}

impl Component for Chat {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let (user, _) = ctx
            .link()
            .context::<User>(Callback::noop())
            .expect("context to be set");
        let wss = WebsocketService::new();
        let username = user.username.borrow().clone();

        let message = WebSocketMessage {
            message_type: MsgTypes::Register,
            data: Some(username.to_string()),
            data_array: None,
        };

        let _ = wss.tx.clone().try_send(serde_json::to_string(&message).unwrap());

        Self {
            users: vec![],
            messages: vec![],
            reactions: BTreeMap::new(),
            threads: BTreeMap::new(),
            selected_thread: None,
            chat_input: NodeRef::default(),
            thread_input: NodeRef::default(),
            wss,
            _producer: EventBus::bridge(ctx.link().callback(Msg::HandleMsg)),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::HandleMsg(s) => {
                let msg: WebSocketMessage = serde_json::from_str(&s).unwrap();
                match msg.message_type {
                    MsgTypes::Users => {
                        let users_from_message = msg.data_array.unwrap_or_default();
                        self.users = users_from_message
                            .iter()
                            .map(|u| UserProfile {
                                name: u.into(),
                                avatar: format!(
                                    "https://avatars.dicebear.com/api/adventurer-neutral/{}.svg",
                                    u
                                )
                                .into(),
                            })
                            .collect();
                        true
                    }
                    MsgTypes::Message => {
                        let message_data: MessageData = serde_json::from_str(&msg.data.unwrap()).unwrap();
                        self.messages.push(ChatMessage {
                            id: message_data.id,
                            from: message_data.from,
                            message: message_data.message,
                        });
                        true
                    }
                    MsgTypes::Reaction => {
                        let reaction_data: ReactionData = serde_json::from_str(&msg.data.unwrap()).unwrap();
                        let entry = self.reactions.entry(reaction_data.message_id).or_default();
                        *entry.entry(reaction_data.emoji).or_insert(0) += 1;
                        true
                    }
                    MsgTypes::Thread => {
                        let thread_data: ThreadData = serde_json::from_str(&msg.data.unwrap()).unwrap();
                        self.threads
                            .entry(thread_data.message_id)
                            .or_default()
                            .push(ThreadReply {
                                author: thread_data.from,
                                message: thread_data.message,
                            });
                        true
                    }
                    _ => false,
                }
            }
            Msg::SubmitMessage => {
                let input = self.chat_input.cast::<HtmlInputElement>();
                if let Some(input) = input {
                    let message = WebSocketMessage {
                        message_type: MsgTypes::Message,
                        data: Some(input.value()),
                        data_array: None,
                    };
                    let _ = self.wss.tx.clone().try_send(serde_json::to_string(&message).unwrap());
                    input.set_value("");
                }
                false
            }
            Msg::SelectThread(id) => {
                self.selected_thread = Some(id);
                true
            }
            Msg::AddReaction(id, emoji) => {
                let reaction_payload = serde_json::json!({
                    "messageId": id,
                    "emoji": emoji,
                });

                let message = WebSocketMessage {
                    message_type: MsgTypes::Reaction,
                    data: Some(reaction_payload.to_string()),
                    data_array: None,
                };

                let _ = self.wss.tx.clone().try_send(serde_json::to_string(&message).unwrap());
                false
            }
            Msg::SubmitThreadReply => {
                if let Some(thread_id) = self.selected_thread {
                    if let Some(input) = self.thread_input.cast::<HtmlInputElement>() {
                        let body = input.value().trim().to_string();
                        if !body.is_empty() {
                            let thread_payload = serde_json::json!({
                                "messageId": thread_id,
                                "message": body,
                            });
                            let message = WebSocketMessage {
                                message_type: MsgTypes::Thread,
                                data: Some(thread_payload.to_string()),
                                data_array: None,
                            };

                            let _ = self.wss.tx.clone().try_send(serde_json::to_string(&message).unwrap());
                            input.set_value("");
                            return false;
                        }
                    }
                }
                false
            }
        }
    }

    fn changed(&mut self, _ctx: &Context<Self>) -> bool {
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let submit = ctx.link().callback(|_| Msg::SubmitMessage);
        let submit_thread = ctx.link().callback(|_| Msg::SubmitThreadReply);
        let active_message = self
            .selected_thread
            .and_then(|id| self.messages.iter().find(|message| message.id == id));

        html! {
            <div class="flex w-screen">
                <div class="flex-none w-56 h-screen bg-gray-100">
                    <div class="text-xl p-3">{"Users"}</div>
                    {
                        self.users.clone().iter().map(|u| {
                            html!{
                                <div class="flex m-3 bg-white rounded-lg p-2">
                                    <div>
                                        <img class="w-12 h-12 rounded-full" src={u.avatar.clone()} alt="avatar"/>
                                    </div>
                                    <div class="flex-grow p-3">
                                        <div class="flex text-xs justify-between">
                                            <div>{u.name.clone()}</div>
                                        </div>
                                        <div class="text-xs text-gray-400">{"Hi there!"}</div>
                                    </div>
                                </div>
                            }
                        }).collect::<Html>()
                    }
                </div>
                <div class="grow h-screen flex flex-col">
                    <div class="w-full h-14 border-b-2 border-gray-300"><div class="text-xl p-3">{"💬 Chat!"}</div></div>
                    <div class="w-full grow overflow-auto border-b-2 border-gray-300 bg-white">
                        {
                            self.messages.iter().map(|m| {
                                let user = self.users.iter().find(|u| u.name == m.from).unwrap();
                                let selected = self.selected_thread == Some(m.id);
                                let reactions = self.reactions.get(&m.id).cloned().unwrap_or_default();
                                let message_id = m.id;
                                let on_select = ctx.link().callback(move |_| Msg::SelectThread(message_id));
                                let reaction_buttons = ["👍", "❤️", "😂", "🎉", "🤔"]
                                    .into_iter()
                                    .map(|emoji| {
                                        let emoji_string = emoji.to_string();
                                        let message_id = message_id;
                                        let add_reaction = ctx.link().callback(move |e: MouseEvent| {
                                            e.stop_propagation();
                                            Msg::AddReaction(message_id, emoji_string.clone())
                                        });
                                        let count = reactions.get(emoji).copied().unwrap_or(0);

                                        html! {
                                            <button
                                                onclick={add_reaction}
                                                class="px-2 py-1 rounded-full bg-white border border-gray-200 text-xs hover:bg-gray-50"
                                                type="button"
                                            >
                                                { format!("{} {}", emoji, if count > 0 { count.to_string() } else { String::new() }) }
                                            </button>
                                        }
                                    })
                                    .collect::<Html>();

                                html!{
                                    <div
                                        class={classes!(
                                            "flex",
                                            "items-end",
                                            "w-5/6",
                                            "bg-gray-100",
                                            "m-8",
                                            "rounded-tl-lg",
                                            "rounded-tr-lg",
                                            "rounded-br-lg",
                                            "cursor-pointer",
                                            "border",
                                            if selected { "border-blue-500 shadow-sm" } else { "border-transparent" },
                                        )}
                                        onclick={on_select}
                                    >
                                        <img class="w-8 h-8 rounded-full m-3" src={user.avatar.clone()} alt="avatar"/>
                                        <div class="p-3">
                                            <div class="text-sm">{m.from.clone()}</div>
                                            <div class="text-xs text-gray-500">
                                                { if m.message.ends_with(".gif") {
                                                    html! { <img class="mt-3" src={m.message.clone()}/> }
                                                } else {
                                                    html! { {m.message.clone()} }
                                                } }
                                            </div>
                                            <div class="mt-3 flex flex-wrap gap-2">{ reaction_buttons }</div>
                                        </div>
                                    </div>
                                }
                            }).collect::<Html>()
                        }
                    </div>
                    <div class="w-full h-14 flex px-3 items-center">
                        <input ref={self.chat_input.clone()} type="text" placeholder="Message" class="block w-full py-2 pl-4 mx-3 bg-gray-100 rounded-full outline-none focus:text-gray-700" name="message" required=true />
                        <button onclick={submit} class="p-3 shadow-sm bg-blue-600 w-10 h-10 rounded-full flex justify-center items-center color-white">
                            <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" class="fill-white">
                                <path d="M0 0h24v24H0z" fill="none"></path><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"></path>
                            </svg>
                        </button>
                    </div>
                </div>
                <div class="w-96 h-screen border-l-2 border-gray-300 bg-gray-50 flex flex-col">
                    <div class="h-14 border-b border-gray-200 px-4 flex items-center">
                        <div class="text-lg font-semibold">{"Thread"}</div>
                    </div>
                    <div class="flex-1 overflow-auto p-4">
                        {
                            if let Some(message) = active_message {
                                let thread_replies = self.threads.get(&message.id).cloned().unwrap_or_default();
                                html! {
                                    <>
                                        <div class="rounded-xl bg-white border border-gray-200 p-4 mb-4 shadow-sm">
                                            <div class="text-xs uppercase tracking-wide text-gray-500">{"Selected message"}</div>
                                            <div class="mt-2 font-semibold text-gray-800">{message.from.clone()}</div>
                                            <div class="mt-1 text-sm text-gray-700">{message.message.clone()}</div>
                                        </div>
                                        <div class="text-xs uppercase tracking-wide text-gray-500 mb-2">{"Replies"}</div>
                                        <div class="space-y-3">
                                            {
                                                thread_replies.iter().map(|reply| {
                                                    html! {
                                                        <div class="rounded-xl bg-white border border-gray-200 p-3 shadow-sm">
                                                            <div class="text-xs text-gray-500">{reply.author.clone()}</div>
                                                            <div class="text-sm text-gray-800 mt-1">{reply.message.clone()}</div>
                                                        </div>
                                                    }
                                                }).collect::<Html>()
                                            }
                                        </div>
                                    </>
                                }
                            } else {
                                html! {
                                    <div class="h-full flex items-center justify-center text-sm text-gray-400 text-center px-6">
                                        {"Click a message to open a thread. Reactions and replies stay visible here."}
                                    </div>
                                }
                            }
                        }
                    </div>
                    <div class="border-t border-gray-200 p-3">
                        <div class="flex gap-2">
                            <input
                                ref={self.thread_input.clone()}
                                type="text"
                                placeholder="Reply in thread"
                                class="block w-full py-2 px-4 bg-white border border-gray-200 rounded-full outline-none focus:text-gray-700"
                                name="threadReply"
                                required=true
                                disabled={self.selected_thread.is_none()}
                            />
                            <button
                                onclick={submit_thread}
                                class="p-3 shadow-sm bg-violet-600 disabled:bg-gray-300 disabled:cursor-not-allowed w-10 h-10 rounded-full flex justify-center items-center color-white"
                                type="button"
                                disabled={self.selected_thread.is_none()}
                            >
                                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" class="fill-white">
                                    <path d="M0 0h24v24H0z" fill="none"></path><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"></path>
                                </svg>
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        }
    }
}
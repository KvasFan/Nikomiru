// SPDX-License-Identifier: GPL-3.0-only

//! AnkiDroid's collection (ANK-055, D-85): the [`Store`] of the in-app
//! AnkiConnect, over AnkiDroid's content provider (`FlashCardsContract`,
//! the provider of AnkiDroid's `AddContentApi`). The first use asks for
//! AnkiDroid's permission; media go through the app's own provider
//! (`MediaProvider` in `packaging/android/java`), which AnkiDroid is allowed
//! to read one file at a time.

use std::path::PathBuf;

use jni::objects::{JObject, JObjectArray, JString, JValue};
use jni::{Env, jni_sig, jni_str};
use nikomiru_anki::bridge::{
    APP_MISSING, Deck, Model, NOT_ALLOWED, Store, StoredNote, Template, join_fields, split_fields,
    split_tags,
};

use crate::platform::android::{app, with_activity};

/// AnkiDroid's provider.
const AUTHORITY: &str = "com.ichi2.anki.flashcards";
/// The permission it asks of other apps.
const PERMISSION: &str = "com.ichi2.anki.permission.READ_WRITE_DATABASE";
/// The app's provider of card media.
const MEDIA_AUTHORITY: &str = "io.github.kvasfan.nikomiru.media";
/// Its folder in the app's cache (as `MediaProvider.FOLDER`).
const MEDIA_FOLDER: &str = "anki-media";
/// `Intent.FLAG_GRANT_READ_URI_PERMISSION`.
const GRANT_READ: i32 = 1;

/// AnkiDroid's collection, reached through Java.
#[derive(Debug, Default)]
pub struct AnkiDroid;

type Jni<T> = jni::errors::Result<T>;

/// The outcome of a step that may find AnkiDroid missing or not allowed.
type Step<T> = Result<T, String>;

fn content(path: &str) -> String {
    format!("content://{AUTHORITY}/{path}")
}

fn string<'l>(env: &mut Env<'l>, object: JObject<'l>) -> Jni<Option<String>> {
    if object.is_null() {
        return Ok(None);
    }
    let text = env.cast_local::<JString>(object)?;
    Ok(Some(text.try_to_string(env)?))
}

fn parse_uri<'l>(env: &mut Env<'l>, text: &str) -> Jni<JObject<'l>> {
    let text = env.new_string(text)?;
    env.call_static_method(
        jni_str!("android/net/Uri"),
        jni_str!("parse"),
        jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
        &[JValue::Object(&text)],
    )?
    .l()
}

fn resolver<'l>(env: &mut Env<'l>, context: &JObject<'_>) -> Jni<JObject<'l>> {
    env.call_method(
        context,
        jni_str!("getContentResolver"),
        jni_sig!("()Landroid/content/ContentResolver;"),
        &[],
    )?
    .l()
}

fn strings<'l>(env: &mut Env<'l>, items: &[&str]) -> Jni<JObjectArray<'l>> {
    let array = env.new_object_array(
        i32::try_from(items.len()).unwrap_or(i32::MAX),
        jni_str!("java/lang/String"),
        JObject::null(),
    )?;
    for (i, item) in items.iter().enumerate() {
        let item = env.new_string(item)?;
        array.set_element(env, i, &item)?;
    }
    Ok(array)
}

/// A value of a `ContentValues`.
enum Put<'a> {
    Text(&'a str),
    Long(i64),
}

fn values<'l>(env: &mut Env<'l>, pairs: &[(&str, Put<'_>)]) -> Jni<JObject<'l>> {
    let values = env.new_object(
        jni_str!("android/content/ContentValues"),
        jni_sig!("()V"),
        &[],
    )?;
    for (key, value) in pairs {
        let key = env.new_string(key)?;
        match value {
            Put::Text(text) => {
                let text = env.new_string(text)?;
                env.call_method(
                    &values,
                    jni_str!("put"),
                    jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                    &[JValue::Object(&key), JValue::Object(&text)],
                )?;
            }
            Put::Long(number) => {
                let boxed = env
                    .call_static_method(
                        jni_str!("java/lang/Long"),
                        jni_str!("valueOf"),
                        jni_sig!("(J)Ljava/lang/Long;"),
                        &[JValue::Long(*number)],
                    )?
                    .l()?;
                env.call_method(
                    &values,
                    jni_str!("put"),
                    jni_sig!("(Ljava/lang/String;Ljava/lang/Long;)V"),
                    &[JValue::Object(&key), JValue::Object(&boxed)],
                )?;
            }
        }
    }
    Ok(values)
}

/// The rows of a query, each the projection's columns as text (`None` for
/// a null); `None` when the provider gave no cursor.
fn query(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    path: &str,
    projection: &[&str],
    selection: Option<&str>,
) -> Jni<Option<Vec<Vec<Option<String>>>>> {
    let resolver = resolver(env, context)?;
    let uri = parse_uri(env, &content(path))?;
    let columns = strings(env, projection)?;
    let selection = match selection {
        Some(s) => JObject::from(env.new_string(s)?),
        None => JObject::null(),
    };
    let cursor = env
        .call_method(
            &resolver,
            jni_str!("query"),
            jni_sig!(
                "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;"
            ),
            &[
                JValue::Object(&uri),
                JValue::Object(&columns),
                JValue::Object(&selection),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?
        .l()?;
    if cursor.is_null() {
        return Ok(None);
    }
    let mut indices = Vec::new();
    for column in projection {
        let name = env.new_string(column)?;
        let index = env
            .call_method(
                &cursor,
                jni_str!("getColumnIndex"),
                jni_sig!("(Ljava/lang/String;)I"),
                &[JValue::Object(&name)],
            )?
            .i()?;
        indices.push(index);
    }
    let mut rows = Vec::new();
    let read = (|| -> Jni<()> {
        while env
            .call_method(&cursor, jni_str!("moveToNext"), jni_sig!("()Z"), &[])?
            .z()?
        {
            let row = env.with_local_frame(2 * indices.len() + 4, |env| -> Jni<_> {
                let mut row = Vec::with_capacity(indices.len());
                for &index in &indices {
                    if index < 0 {
                        row.push(None);
                        continue;
                    }
                    let value = env
                        .call_method(
                            &cursor,
                            jni_str!("getString"),
                            jni_sig!("(I)Ljava/lang/String;"),
                            &[JValue::Int(index)],
                        )?
                        .l()?;
                    row.push(string(env, value)?);
                }
                Ok(row)
            })?;
            rows.push(row);
        }
        Ok(())
    })();
    env.call_method(&cursor, jni_str!("close"), jni_sig!("()V"), &[])?;
    read?;
    Ok(Some(rows))
}

/// Inserts into a provider and returns the new row's address.
fn insert(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    uri: &str,
    pairs: &[(&str, Put<'_>)],
) -> Jni<Option<String>> {
    let resolver = resolver(env, context)?;
    let uri = parse_uri(env, uri)?;
    let values = values(env, pairs)?;
    let inserted = env
        .call_method(
            &resolver,
            jni_str!("insert"),
            jni_sig!("(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;"),
            &[JValue::Object(&uri), JValue::Object(&values)],
        )?
        .l()?;
    if inserted.is_null() {
        return Ok(None);
    }
    let text = env
        .call_method(
            &inserted,
            jni_str!("toString"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )?
        .l()?;
    string(env, text)
}

fn update(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    path: &str,
    pairs: &[(&str, Put<'_>)],
) -> Jni<i32> {
    let resolver = resolver(env, context)?;
    let uri = parse_uri(env, &content(path))?;
    let values = values(env, pairs)?;
    env.call_method(
        &resolver,
        jni_str!("update"),
        jni_sig!("(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I"),
        &[
            JValue::Object(&uri),
            JValue::Object(&values),
            JValue::Object(&JObject::null()),
            JValue::Object(&JObject::null()),
        ],
    )?
    .i()
}

/// The last part of an address (`…/notes/42` → `42`).
fn last(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or_default()
}

fn id_of(uri: Option<String>, what: &str) -> Step<i64> {
    let uri = uri.ok_or_else(|| format!("AnkiDroid did not add the {what}"))?;
    last(&uri)
        .parse()
        .map_err(|_| format!("AnkiDroid answered {uri} for the {what}"))
}

/// AnkiDroid's package, or [`APP_MISSING`]; asks for the permission (and
/// answers [`NOT_ALLOWED`]) until it is given.
fn ready(env: &mut Env<'_>, context: &JObject<'_>) -> Jni<Step<String>> {
    let manager = env
        .call_method(
            context,
            jni_str!("getPackageManager"),
            jni_sig!("()Landroid/content/pm/PackageManager;"),
            &[],
        )?
        .l()?;
    let authority = env.new_string(AUTHORITY)?;
    let provider = env
        .call_method(
            &manager,
            jni_str!("resolveContentProvider"),
            jni_sig!("(Ljava/lang/String;I)Landroid/content/pm/ProviderInfo;"),
            &[JValue::Object(&authority), JValue::Int(0)],
        )?
        .l()?;
    if provider.is_null() {
        return Ok(Err(APP_MISSING.to_string()));
    }
    let package = env
        .get_field(
            &provider,
            jni_str!("packageName"),
            jni_sig!("Ljava/lang/String;"),
        )?
        .l()?;
    let package = string(env, package)?.unwrap_or_default();
    let permission = env.new_string(PERMISSION)?;
    let granted = env
        .call_method(
            context,
            jni_str!("checkSelfPermission"),
            jni_sig!("(Ljava/lang/String;)I"),
            &[JValue::Object(&permission)],
        )?
        .i()?;
    if granted != 0 {
        ask_permission();
        return Ok(Err(NOT_ALLOWED.to_string()));
    }
    Ok(Ok(package))
}

/// Shows Android's request for AnkiDroid's permission (on Java's main
/// thread, as for the screen, `inhibit.rs`).
fn ask_permission() {
    let Some(app) = app() else {
        return;
    };
    app.run_on_java_main_thread(Box::new(|| {
        let asked = with_activity(|env, activity| {
            let wanted = strings(env, &[PERMISSION])?;
            env.call_method(
                activity,
                jni_str!("requestPermissions"),
                jni_sig!("([Ljava/lang/String;I)V"),
                &[JValue::Object(&wanted), JValue::Int(60)],
            )?;
            Ok(())
        });
        if let Err(e) = asked {
            tracing::warn!(target: "anki", "asking for AnkiDroid's permission: {e}");
        }
    }));
}

/// Runs `act` once AnkiDroid is there and allowed; Java's exceptions become
/// messages.
fn with_ankidroid<T>(
    act: impl FnOnce(&mut Env<'_>, &JObject<'_>, &str) -> Jni<Step<T>>,
) -> Step<T> {
    with_activity(|env, context| match ready(env, context)? {
        Ok(package) => act(env, context, &package),
        Err(e) => Ok(Err(e)),
    })
    .and_then(|step| step)
}

fn number(text: Option<&Option<String>>) -> Option<i64> {
    text?.as_deref()?.parse().ok()
}

fn text(cell: Option<&Option<String>>) -> String {
    cell.cloned().flatten().unwrap_or_default()
}

/// The app's cache folder, which `MediaProvider` serves from.
fn cache_dir(env: &mut Env<'_>, context: &JObject<'_>) -> Jni<PathBuf> {
    let dir = env
        .call_method(
            context,
            jni_str!("getCacheDir"),
            jni_sig!("()Ljava/io/File;"),
            &[],
        )?
        .l()?;
    let path = env
        .call_method(
            &dir,
            jni_str!("getAbsolutePath"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )?
        .l()?;
    Ok(PathBuf::from(string(env, path)?.unwrap_or_default()))
}

/// A media name the app's provider serves: one plain file name.
fn plain_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim_start_matches('.');
    if name.is_empty() {
        "media".to_string()
    } else {
        name.to_string()
    }
}

fn grant(
    env: &mut Env<'_>,
    context: &JObject<'_>,
    package: &str,
    uri: &JObject<'_>,
    give: bool,
) -> Jni<()> {
    let package = env.new_string(package)?;
    if give {
        env.call_method(
            context,
            jni_str!("grantUriPermission"),
            jni_sig!("(Ljava/lang/String;Landroid/net/Uri;I)V"),
            &[
                JValue::Object(&package),
                JValue::Object(uri),
                JValue::Int(GRANT_READ),
            ],
        )?;
    } else {
        env.call_method(
            context,
            jni_str!("revokeUriPermission"),
            jni_sig!("(Ljava/lang/String;Landroid/net/Uri;I)V"),
            &[
                JValue::Object(&package),
                JValue::Object(uri),
                JValue::Int(GRANT_READ),
            ],
        )?;
    }
    Ok(())
}

impl Store for AnkiDroid {
    fn decks(&self) -> Result<Vec<Deck>, String> {
        with_ankidroid(|env, context, _| {
            let rows = query(env, context, "decks", &["deck_id", "deck_name"], None)?;
            Ok(Ok(rows
                .unwrap_or_default()
                .iter()
                .filter_map(|row| {
                    Some(Deck {
                        id: number(row.first())?,
                        name: text(row.get(1)),
                    })
                })
                .collect()))
        })
    }

    fn add_deck(&self, name: &str) -> Result<i64, String> {
        with_ankidroid(|env, context, _| {
            let uri = insert(
                env,
                context,
                &content("decks"),
                &[("deck_name", Put::Text(name))],
            )?;
            Ok(id_of(uri, "deck"))
        })
    }

    fn models(&self) -> Result<Vec<Model>, String> {
        with_ankidroid(|env, context, _| {
            let rows = query(
                env,
                context,
                "models",
                &["_id", "name", "field_names"],
                None,
            )?;
            Ok(Ok(rows
                .unwrap_or_default()
                .iter()
                .filter_map(|row| {
                    Some(Model {
                        id: number(row.first())?,
                        name: text(row.get(1)),
                        fields: split_fields(&text(row.get(2))),
                    })
                })
                .collect()))
        })
    }

    fn add_model(
        &self,
        name: &str,
        fields: &[String],
        css: &str,
        templates: &[Template],
    ) -> Result<i64, String> {
        with_ankidroid(|env, context, _| {
            let joined = join_fields(fields);
            let uri = insert(
                env,
                context,
                &content("models"),
                &[
                    ("name", Put::Text(name)),
                    ("field_names", Put::Text(&joined)),
                    ("num_cards", Put::Long(templates.len() as i64)),
                    ("css", Put::Text(css)),
                    ("sort_field_index", Put::Long(0)),
                ],
            )?;
            let id = match id_of(uri, "note type") {
                Ok(id) => id,
                Err(e) => return Ok(Err(e)),
            };
            for (ord, template) in templates.iter().enumerate() {
                update(
                    env,
                    context,
                    &format!("models/{id}/templates/{ord}"),
                    &[
                        ("card_template_name", Put::Text(&template.name)),
                        ("question_format", Put::Text(&template.front)),
                        ("answer_format", Put::Text(&template.back)),
                    ],
                )?;
            }
            Ok(Ok(id))
        })
    }

    fn find_notes(&self, search: &str) -> Result<Vec<i64>, String> {
        with_ankidroid(|env, context, _| {
            let rows = query(env, context, "notes", &["_id"], Some(search))?;
            Ok(Ok(rows
                .unwrap_or_default()
                .iter()
                .filter_map(|row| number(row.first()))
                .collect()))
        })
    }

    fn note(&self, id: i64) -> Result<Option<StoredNote>, String> {
        let found = with_ankidroid(|env, context, _| {
            let rows = query(
                env,
                context,
                &format!("notes/{id}"),
                &["_id", "mid", "flds", "tags"],
                None,
            )?;
            Ok(Ok(rows.unwrap_or_default().first().and_then(|row| {
                Some(StoredNote {
                    id: number(row.first())?,
                    model: number(row.get(1))?,
                    fields: split_fields(&text(row.get(2))),
                    tags: split_tags(&text(row.get(3))),
                })
            })))
        });
        match found {
            // a note that is gone is an exception of AnkiDroid's backend
            Err(e) if e.contains("NotFound") || e.contains("not found") => Ok(None),
            other => other,
        }
    }

    fn add_note(
        &self,
        model: i64,
        deck: i64,
        fields: &[String],
        tags: &[String],
    ) -> Result<i64, String> {
        with_ankidroid(|env, context, _| {
            let joined = join_fields(fields);
            let tags = tags.join(" ");
            let uri = insert(
                env,
                context,
                &content("notes"),
                &[
                    ("mid", Put::Long(model)),
                    ("flds", Put::Text(&joined)),
                    ("tags", Put::Text(&tags)),
                ],
            )?;
            let id = match id_of(uri, "note") {
                Ok(id) => id,
                Err(e) => return Ok(Err(e)),
            };
            // its cards into the deck, as AnkiDroid's AddContentApi does
            let cards = query(env, context, &format!("notes/{id}/cards"), &["ord"], None)?;
            for row in cards.unwrap_or_default() {
                if let Some(ord) = number(row.first()) {
                    update(
                        env,
                        context,
                        &format!("notes/{id}/cards/{ord}"),
                        &[("deck_id", Put::Long(deck))],
                    )?;
                }
            }
            Ok(Ok(id))
        })
    }

    fn set_fields(&self, id: i64, fields: &[String]) -> Result<(), String> {
        with_ankidroid(|env, context, _| {
            let joined = join_fields(fields);
            update(
                env,
                context,
                &format!("notes/{id}"),
                &[("flds", Put::Text(&joined))],
            )?;
            Ok(Ok(()))
        })
    }

    fn set_tags(&self, id: i64, tags: &[String]) -> Result<(), String> {
        with_ankidroid(|env, context, _| {
            let tags = tags.join(" ");
            update(
                env,
                context,
                &format!("notes/{id}"),
                &[("tags", Put::Text(&tags))],
            )?;
            Ok(Ok(()))
        })
    }

    fn delete_note(&self, id: i64) -> Result<(), String> {
        with_ankidroid(|env, context, _| {
            let resolver = resolver(env, context)?;
            let uri = parse_uri(env, &content(&format!("notes/{id}")))?;
            env.call_method(
                &resolver,
                jni_str!("delete"),
                jni_sig!("(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I"),
                &[
                    JValue::Object(&uri),
                    JValue::Object(&JObject::null()),
                    JValue::Object(&JObject::null()),
                ],
            )?;
            Ok(Ok(()))
        })
    }

    fn store_media(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        with_ankidroid(|env, context, package| {
            let folder = cache_dir(env, context)?.join(MEDIA_FOLDER);
            let name = plain_name(name);
            let path = folder.join(&name);
            if let Err(e) =
                std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&path, bytes))
            {
                return Ok(Err(format!("{}: {e}", path.display())));
            }
            let address = format!("content://{MEDIA_AUTHORITY}/{name}");
            let uri = parse_uri(env, &address)?;
            grant(env, context, package, &uri, true)?;
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
            let stored = insert(
                env,
                context,
                &content("media"),
                &[
                    ("file_uri", Put::Text(&address)),
                    ("preferred_name", Put::Text(stem)),
                ],
            );
            // AnkiDroid has copied it (or failed): the file is no longer needed
            let revoked = grant(env, context, package, &uri, false);
            let _ = std::fs::remove_file(&path);
            let stored = stored?;
            revoked?;
            Ok(stored
                .map(|uri| last(&uri).to_string())
                .filter(|n| !n.is_empty())
                .ok_or_else(|| format!("AnkiDroid did not take {name}")))
        })
    }

    fn browse(&self, _query: &str) -> Result<(), String> {
        with_ankidroid(|env, context, package| {
            let manager = env
                .call_method(
                    context,
                    jni_str!("getPackageManager"),
                    jni_sig!("()Landroid/content/pm/PackageManager;"),
                    &[],
                )?
                .l()?;
            let package = env.new_string(package)?;
            let intent = env
                .call_method(
                    &manager,
                    jni_str!("getLaunchIntentForPackage"),
                    jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"),
                    &[JValue::Object(&package)],
                )?
                .l()?;
            if intent.is_null() {
                return Ok(Err(APP_MISSING.to_string()));
            }
            env.call_method(
                context,
                jni_str!("startActivity"),
                jni_sig!("(Landroid/content/Intent;)V"),
                &[JValue::Object(&intent)],
            )?;
            Ok(Ok(()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_names_stay_in_the_folder() {
        assert_eq!(plain_name("a/../b.m4a"), "a_.._b.m4a");
        assert_eq!(plain_name("..hidden.jpg"), "hidden.jpg");
        assert_eq!(plain_name(""), "media");
        assert_eq!(last("content://x/notes/42"), "42");
    }
}

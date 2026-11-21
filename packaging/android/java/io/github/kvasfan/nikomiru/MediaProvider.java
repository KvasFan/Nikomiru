// SPDX-License-Identifier: GPL-3.0-only

package io.github.kvasfan.nikomiru;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;
import java.util.List;

/**
 * The card media handed to AnkiDroid (ANK-055): AnkiDroid reads a sound or
 * picture through a {@code content://} address it was given permission to
 * read, so the files the app writes to {@code cache/anki-media} are served
 * from here, read only. Nothing else of the app is reachable: the provider
 * is not exported and serves no folder but that one.
 */
public final class MediaProvider extends ContentProvider {
    /** The folder, under the app's cache, the files are in. */
    private static final String FOLDER = "anki-media";

    @Override
    public boolean onCreate() {
        return true;
    }

    /** The file of an address: one plain name in the folder, or nothing. */
    private File file(Uri uri) throws FileNotFoundException {
        List<String> segments = uri.getPathSegments();
        if (segments.size() != 1) {
            throw new FileNotFoundException(uri.toString());
        }
        String name = segments.get(0);
        if (name.isEmpty() || name.startsWith(".") || name.contains("/")) {
            throw new FileNotFoundException(uri.toString());
        }
        File file = new File(new File(getContext().getCacheDir(), FOLDER), name);
        if (!file.isFile()) {
            throw new FileNotFoundException(uri.toString());
        }
        return file;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) {
            throw new SecurityException("read only");
        }
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public String getType(Uri uri) {
        String name = uri.getLastPathSegment();
        if (name == null) {
            return null;
        }
        int dot = name.lastIndexOf('.');
        String extension = dot < 0 ? "" : name.substring(dot + 1).toLowerCase(java.util.Locale.ROOT);
        switch (extension) {
            case "m4a":
                return "audio/mp4";
            case "ogg":
            case "opus":
                return "audio/ogg";
            case "mp3":
                return "audio/mpeg";
            case "jpg":
            case "jpeg":
                return "image/jpeg";
            case "png":
                return "image/png";
            case "webp":
                return "image/webp";
            case "avif":
                return "image/avif";
            case "webm":
                return "video/webm";
            default:
                return "application/octet-stream";
        }
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] selectionArgs,
            String sortOrder) {
        File file;
        try {
            file = file(uri);
        } catch (FileNotFoundException e) {
            return null;
        }
        MatrixCursor cursor = new MatrixCursor(
                new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE});
        cursor.addRow(new Object[] {file.getName(), file.length()});
        return cursor;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException("read only");
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException("read only");
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException("read only");
    }
}

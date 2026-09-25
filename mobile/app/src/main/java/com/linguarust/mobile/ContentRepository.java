package com.linguarust.mobile;

import android.content.res.AssetManager;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Загрузка общих данных проекта из assets. */
public final class ContentRepository {
    private final List<DictionaryEntry> dictionary;
    private final List<GrammarEntry> grammar;
    private final List<ReadingText> texts;

    public ContentRepository(AssetManager assets) {
        dictionary = loadDictionary(assets);
        grammar = loadGrammar(assets);
        texts = loadTexts(assets);
    }

    public List<DictionaryEntry> dictionary() {
        return dictionary;
    }

    public List<GrammarEntry> grammar() {
        return grammar;
    }

    public List<ReadingText> texts() {
        return texts;
    }

    public List<DictionaryEntry> search(String query) {
        if (query == null || query.trim().isEmpty()) return dictionary;
        List<DictionaryEntry> result = new ArrayList<>();
        for (DictionaryEntry entry : dictionary) {
            if (entry.matches(query)) result.add(entry);
        }
        return result;
    }

    public DictionaryEntry wordOfDay(int dayOfYear) {
        if (dictionary.isEmpty()) return null;
        int index = Math.floorMod(dayOfYear, dictionary.size());
        return dictionary.get(index);
    }

    private static List<DictionaryEntry> loadDictionary(AssetManager assets) {
        List<DictionaryEntry> result = new ArrayList<>();
        try (BufferedReader reader = reader(assets, "dictionary.tsv")) {
            String line;
            while ((line = reader.readLine()) != null) {
                if (line.trim().isEmpty() || line.startsWith("#")) continue;
                String[] parts = line.split("\\t", 3);
                if (parts.length < 2) continue;
                result.add(new DictionaryEntry(
                        parts[0].trim(),
                        parts[1].trim(),
                        parts.length == 2 ? "" : parts[2].trim()
                ));
            }
        } catch (IOException | RuntimeException ignored) {
            // Экран покажет понятное пустое состояние, если assets повреждены.
        }
        return Collections.unmodifiableList(result);
    }

    private static List<GrammarEntry> loadGrammar(AssetManager assets) {
        List<GrammarEntry> result = new ArrayList<>();
        try {
            JSONArray array = new JSONArray(readAsset(assets, "grammar.json"));
            for (int i = 0; i < array.length(); i++) {
                JSONObject object = array.getJSONObject(i);
                JSONArray rawOptions = object.getJSONArray("options");
                List<String> options = new ArrayList<>();
                for (int j = 0; j < rawOptions.length(); j++) {
                    options.add(rawOptions.getString(j));
                }
                result.add(new GrammarEntry(
                        object.optString("topic", "Grammar"),
                        object.optString("prompt", ""),
                        options,
                        object.optInt("correct_index", 0),
                        object.optString("explanation", "")
                ));
            }
        } catch (Exception ignored) {
            // Не блокируем приложение из-за одного повреждённого файла.
        }
        return Collections.unmodifiableList(result);
    }

    private static List<ReadingText> loadTexts(AssetManager assets) {
        List<ReadingText> result = new ArrayList<>();
        try {
            JSONArray array = new JSONArray(readAsset(assets, "texts.json"));
            for (int i = 0; i < array.length(); i++) {
                JSONObject object = array.getJSONObject(i);
                result.add(new ReadingText(
                        object.optString("title", "Текст"),
                        object.optString("level", ""),
                        object.optString("summary", ""),
                        object.optString("content", "")
                ));
            }
        } catch (Exception ignored) {
            // Чтение — необязательный экран.
        }
        return Collections.unmodifiableList(result);
    }

    private static BufferedReader reader(AssetManager assets, String name) throws IOException {
        InputStream stream = assets.open(name);
        return new BufferedReader(new InputStreamReader(stream, StandardCharsets.UTF_8));
    }

    private static String readAsset(AssetManager assets, String name) throws IOException {
        try (InputStream stream = assets.open(name);
             BufferedReader reader = new BufferedReader(
                     new InputStreamReader(stream, StandardCharsets.UTF_8))) {
            StringBuilder result = new StringBuilder();
            String line;
            while ((line = reader.readLine()) != null) result.append(line).append('\n');
            return result.toString();
        }
    }
}

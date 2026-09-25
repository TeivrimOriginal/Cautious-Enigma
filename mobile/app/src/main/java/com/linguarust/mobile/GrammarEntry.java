package com.linguarust.mobile;

import java.util.Collections;
import java.util.List;

/** Грамматическое упражнение из общего JSON-файла проекта. */
public final class GrammarEntry {
    public final String topic;
    public final String prompt;
    public final List<String> options;
    public final int correctIndex;
    public final String explanation;

    public GrammarEntry(
            String topic,
            String prompt,
            List<String> options,
            int correctIndex,
            String explanation
    ) {
        this.topic = topic;
        this.prompt = prompt;
        this.options = Collections.unmodifiableList(options);
        this.correctIndex = correctIndex;
        this.explanation = explanation == null ? "" : explanation;
    }
}

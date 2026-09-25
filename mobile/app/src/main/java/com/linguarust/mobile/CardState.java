package com.linguarust.mobile;

import org.json.JSONException;
import org.json.JSONObject;

/** Состояние одной карточки и её SM-2 состояние. */
public final class CardState {
    public String front;
    public String back;
    public String example;
    public int repetitions;
    public int intervalDays;
    public double ease;
    public long dueAt;
    public long lastReviewedAt;

    public CardState(String front, String back, String example) {
        this.front = front;
        this.back = back;
        this.example = example == null ? "" : example;
        this.repetitions = 0;
        this.intervalDays = 0;
        this.ease = 2.5;
        this.dueAt = 0L;
        this.lastReviewedAt = 0L;
    }

    public boolean isDue(long now) {
        return dueAt <= now;
    }

    public int masteryPercent() {
        return Math.min(100, Math.max(0, repetitions * 20));
    }

    public JSONObject toJson() throws JSONException {
        JSONObject object = new JSONObject();
        object.put("front", front);
        object.put("back", back);
        object.put("example", example);
        object.put("repetitions", repetitions);
        object.put("intervalDays", intervalDays);
        object.put("ease", ease);
        object.put("dueAt", dueAt);
        object.put("lastReviewedAt", lastReviewedAt);
        return object;
    }

    public static CardState fromJson(JSONObject object) {
        CardState card = new CardState(
                object.optString("front", ""),
                object.optString("back", ""),
                object.optString("example", "")
        );
        card.repetitions = object.optInt("repetitions", 0);
        card.intervalDays = object.optInt("intervalDays", 0);
        card.ease = object.optDouble("ease", 2.5);
        card.dueAt = object.optLong("dueAt", 0L);
        card.lastReviewedAt = object.optLong("lastReviewedAt", 0L);
        return card;
    }
}

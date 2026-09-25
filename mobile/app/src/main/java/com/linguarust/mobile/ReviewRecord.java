package com.linguarust.mobile;

/** Одна запись журнала повторения карточки. */
public final class ReviewRecord {
    public final long timestamp;
    public final int quality;

    public ReviewRecord(long timestamp, int quality) {
        this.timestamp = timestamp;
        this.quality = quality;
    }

    public boolean successful() {
        return quality >= 3;
    }
}

package com.linguarust.mobile;

/** Java-порция алгоритма SM-2, совпадающего с src/sm2.rs. */
public final class Sm2 {
    public static final long DAY_MS = 24L * 60L * 60L * 1000L;
    public static final double MIN_EASE = 1.3;

    private Sm2() {
    }

    public static void apply(CardState card, int quality, long now) {
        int q = Math.max(0, Math.min(5, quality));
        if (q >= 3) {
            card.repetitions++;
            if (card.repetitions == 1) {
                card.intervalDays = 1;
            } else if (card.repetitions == 2) {
                card.intervalDays = 6;
            } else {
                card.intervalDays = Math.max(
                        1,
                        (int) Math.round(card.intervalDays * card.ease)
                );
            }
        } else {
            card.repetitions = 0;
            card.intervalDays = 1;
        }

        double delta = 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02);
        card.ease = Math.max(MIN_EASE, card.ease + delta);
        card.lastReviewedAt = now;
        card.dueAt = startOfDay(now) + card.intervalDays * DAY_MS;
    }

    public static long startOfDay(long timestamp) {
        java.util.Calendar calendar = java.util.Calendar.getInstance();
        calendar.setTimeInMillis(timestamp);
        calendar.set(java.util.Calendar.HOUR_OF_DAY, 0);
        calendar.set(java.util.Calendar.MINUTE, 0);
        calendar.set(java.util.Calendar.SECOND, 0);
        calendar.set(java.util.Calendar.MILLISECOND, 0);
        return calendar.getTimeInMillis();
    }
}

# LinguaRust Mobile

Нативная офлайн-версия LinguaRust для Android. Проект сделан на Java без внешних
библиотек: его можно открыть и собирать в установленной Android Studio.

## Что внутри

- SM-2 повторение карточек с оценками «Опять», «Сложно», «Нормально», «Легко».
- Локальный словарь 340 слов с поиском по английскому и русскому.
- Добавление слов из словаря и из экрана «Слово дня».
- XP, уровни, дневная цель, серия дней и точность.
- 27 грамматических упражнений с объяснением ошибки.
- Чтение четырёх текстов A2–B2.
- Статистика и локальный сброс прогресса.
- История повторений каждой карточки с датой, результатом и начисленным XP.
- Переключатель интерфейса русский/английский с сохранением выбора.
- Сравнение двух карточек: перевод, пример, серия повторений и интервал.
- Экзамен: 10 вопросов EN↔RU за 90 секунд, счёт и XP за ответы.
- Тёмная тема через ресурсы Android.
- Данные хранятся только на устройстве в `SharedPreferences`; регистрация и сеть
  для этой версии не нужны.

Словарь, тексты и упражнения берутся из общей папки `../data` на этапе сборки,
поэтому мобильная версия использует тот же учебный контент, что и сайт.

## Открыть в Android Studio

1. В Android Studio выбери **Open**.
2. Открой папку `mobile` этого репозитория.
3. Дождись Gradle Sync и выбери SDK Android 36.
4. Запусти конфигурацию `app`.

Проект использует Java 17, совместимую с Java 21 из Android Studio.

## Сборка APK из терминала

Из папки `mobile`:

```powershell
.\build-apk.ps1
```

Скрипт автоматически находит Java 21 и Android SDK на этой машине. Эквивалент вручную:

```powershell
$env:JAVA_HOME = "D:\12344\jbr"
$env:ANDROID_HOME = "C:\Users\teivrim\AppData\Local\Android\Sdk"
.\gradlew.bat :app:assembleDebug
```

Готовый файл:

```text
mobile/app/build/outputs/apk/debug/app-debug.apk
```

## Установка на USB-устройство

Включи на телефоне **Для разработчиков → Отлаживание по USB**, подтверди отпечаток
компьютера и подключи устройство. Затем:

```powershell
$adb = "C:\Users\teivrim\AppData\Local\Android\Sdk\platform-tools\adb.exe"
& $adb devices
& $adb install -r app\build\outputs\apk\debug\app-debug.apk
```

После установки LinguaRust появится в списке приложений под именем **LinguaRust**.
Эмулятор для сборки не нужен — он может понадобиться только для тестирования
без физического телефона.

## Структура

```text
mobile/
├── app/src/main/java/com/linguarust/mobile/
│   ├── MainActivity.java       навигация и экраны
│   ├── MobileStore.java        SharedPreferences, XP и статистика
│   ├── Sm2.java                алгоритм интервального повторения
│   ├── ContentRepository.java  загрузка data/
│   └── модели Dictionary/Card/Grammar/Reading
├── app/src/main/res/           тема и ресурсы Android
├── gradlew / gradlew.bat       Gradle Wrapper
└── app/build.gradle            настройки Android-модуля
```

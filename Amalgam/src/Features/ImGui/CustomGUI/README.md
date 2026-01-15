# Custom GUI Integration

Этот модуль интегрирует кастомный дизайн GUI из папки `gui/` в чит Amalgam.

## Как включить кастомное меню

Для использования нового дизайна меню, добавьте define `USE_CUSTOM_GUI` в настройках проекта:

1. Откройте свойства проекта в Visual Studio
2. Перейдите в C/C++ -> Preprocessor -> Preprocessor Definitions
3. Добавьте `USE_CUSTOM_GUI`

Или добавьте в начало файла `Amalgam/src/Features/ImGui/Render.cpp`:
```cpp
#define USE_CUSTOM_GUI
```

## Структура файлов

- `CustomGUI.h` / `CustomGUI.cpp` - Кастомные элементы интерфейса (checkbox, slider, combo и т.д.)
- `CustomMenu.h` / `CustomMenu.cpp` - Основное меню с интеграцией функций чита

## Элементы GUI

Доступные элементы:
- `CustomGUI::Checkbox` - Чекбокс
- `CustomGUI::Button` - Кнопка
- `CustomGUI::SliderFloat` / `CustomGUI::SliderInt` - Слайдеры
- `CustomGUI::Combo` / `CustomGUI::MultiCombo` - Выпадающие списки
- `CustomGUI::InputText` - Текстовое поле
- `CustomGUI::KeyBind` - Привязка клавиш
- `CustomGUI::Child` - Контейнер с заголовком
- `CustomGUI::Tabs` - Система вкладок
- `CustomGUI::Spinner` - Индикатор загрузки

## Интеграция с переменными чита

Все элементы поддерживают работу напрямую с `ConfigVar<T>`:

```cpp
CustomGUI::Checkbox("Auto shoot", Vars::Aimbot::General::AutoShoot);
CustomGUI::SliderFloat("Aim FOV", Vars::Aimbot::General::AimFOV);
CustomGUI::Combo("Aim type", Vars::Aimbot::General::AimType);
```

## Клавиши

- `INSERT` или `F3` - Открыть/закрыть меню (настраивается в Vars::Menu::PrimaryKey и SecondaryKey)

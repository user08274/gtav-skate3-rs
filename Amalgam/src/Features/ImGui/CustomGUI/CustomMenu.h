#pragma once
#include "CustomGUI.h"

class CCustomMenu
{
private:
    // Tab indices
    int m_iCurrentTab = 0;
    int m_iCurrentSubTab = 0;

    // Tab content functions
    void DrawLegitTab();
    void DrawVisualsTab();
    void DrawMiscTab();
    void DrawMovementTab();
    void DrawSkinsTab();
    void DrawConfigTab();
    void DrawPlayersTab();

    // Sub-tab content
    void DrawLegitGeneral();
    void DrawLegitRage();
    void DrawLegitTrigger();

    void DrawVisualsEntities();
    void DrawVisualsGame();
    void DrawVisualsScreen();

    void DrawMiscGeneral();

    void DrawMovementGeneral();
    void DrawMovementRecorder();
    void DrawMovementCalculator();

    void DrawConfigGeneral();

public:
    void Draw();
    void Initialize();

    bool m_bIsOpen = false;
};

ADD_FEATURE(CCustomMenu, CustomMenu);

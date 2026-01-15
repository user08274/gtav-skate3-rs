#include "CustomMenu.h"
#include "../Render.h"

void CCustomMenu::Initialize()
{
    // Fonts will be initialized on first draw when they are available
}

void CCustomMenu::Draw()
{
    using namespace ImGui;
    
    // Check display size
    if (!(GetIO().DisplaySize.x > 160.f && GetIO().DisplaySize.y > 28.f))
        return;
    
    // Initialize fonts if not done yet (fonts are loaded after Initialize is called)
    static bool bFontsInitialized = false;
    if (!bFontsInitialized && F::Render.FontRegular)
    {
        CustomGUI::Fonts::Initialize();
        bFontsInitialized = true;
    }
    
    // Store key states - REQUIRED for KeyHandler to work properly
    if (m_bIsOpen)
    {
        for (int iKey = 0; iKey < 256; iKey++)
            U::KeyHandler.StoreKey(iKey);
    }
    else
    {
        U::KeyHandler.StoreKey(Vars::Menu::PrimaryKey.Value);
        U::KeyHandler.StoreKey(Vars::Menu::SecondaryKey.Value);
    }
    
    // Handle menu toggle
    if (U::KeyHandler.Pressed(Vars::Menu::PrimaryKey.Value) || U::KeyHandler.Pressed(Vars::Menu::SecondaryKey.Value))
    {
        m_bIsOpen = !m_bIsOpen;
        I::MatSystemSurface->SetCursorAlwaysVisible(m_bIsOpen);
    }

    if (!m_bIsOpen)
        return;

    CustomGUI::Colors::LoadFromVars();

    SetNextWindowSize({ 630.f, 500.f }, ImGuiCond_Always);
    Begin("Amalgam", nullptr,
        ImGuiWindowFlags_NoTitleBar |
        ImGuiWindowFlags_NoBackground |
        ImGuiWindowFlags_NoScrollbar |
        ImGuiWindowFlags_NoCollapse |
        ImGuiWindowFlags_NoResize);
    {
        ImVec2 vWindowPos = GetWindowPos();
        ImVec2 vWindowSize = GetWindowSize();
        ImVec2 vWindowEnd = { vWindowPos.x + vWindowSize.x, vWindowPos.y + vWindowSize.y };

        // Draw background
        PushClipRect(vWindowPos, vWindowEnd, false);
        GetBackgroundDrawList()->AddRectFilled(vWindowPos, vWindowEnd,
            CustomGUI::Colors::Background, 6.0f, ImDrawFlags_RoundCornersAll);
        GetBackgroundDrawList()->AddRect(vWindowPos, vWindowEnd,
            CustomGUI::Colors::Border, 6.0f, ImDrawFlags_RoundCornersAll);
        
        // Draw title
        if (CustomGUI::Fonts::Title)
        {
            ImVec2 vTitlePos = { vWindowPos.x + 30.0f, vWindowPos.y + 10.0f };
            GetBackgroundDrawList()->AddText(CustomGUI::Fonts::Title, CustomGUI::Fonts::Title->FontSize,
                vTitlePos, CustomGUI::Colors::Accent, "A");
        }
        PopClipRect();

        // Define tabs
        std::vector<CustomGUI::Tab_t> vTabs = {
            { "aimbot", "B", {"general", "hitscan", "projectile"} },
            { "visuals", "A", {"esp", "game", "screen"} },
            { "misc", "C", {"main", "hvh"} },
            { "movement", "O", {"general", "edgebug"} },
            { "config", "G", {"general", "binds"} },
            { "players", "E", {"list"} }
        };

        // Draw tabs
        CustomGUI::Tabs(vTabs, m_iCurrentTab, m_iCurrentSubTab);

        // Draw content area
        SetCursorPos({ 150.0f, 10.0f });

        switch (m_iCurrentTab)
        {
        case 0: DrawLegitTab(); break;
        case 1: DrawVisualsTab(); break;
        case 2: DrawMiscTab(); break;
        case 3: DrawMovementTab(); break;
        case 4: DrawConfigTab(); break;
        case 5: DrawPlayersTab(); break;
        }
        
        // Set cursor type for rendering
        F::Render.Cursor = GetMouseCursor();
    }
    End();
}

void CCustomMenu::DrawLegitTab()
{
    switch (m_iCurrentSubTab)
    {
    case 0: DrawLegitGeneral(); break;
    case 1: DrawLegitRage(); break;
    case 2: DrawLegitTrigger(); break;
    }
}

void CCustomMenu::DrawLegitGeneral()
{
    CustomGUI::Child("Aimbot", { 230.0f, 310.0f }, []()
    {
        CustomGUI::Combo("Aim type", Vars::Aimbot::General::AimType);
        CustomGUI::Combo("Target selection", Vars::Aimbot::General::TargetSelection);
        CustomGUI::MultiCombo("Targets", Vars::Aimbot::General::Target);
        CustomGUI::MultiCombo("Ignore", Vars::Aimbot::General::Ignore);
        CustomGUI::SliderFloat("Aim FOV", Vars::Aimbot::General::AimFOV);
        CustomGUI::SliderInt("Max targets", Vars::Aimbot::General::MaxTargets);
        CustomGUI::Checkbox("Auto shoot", Vars::Aimbot::General::AutoShoot);
        CustomGUI::Checkbox("FOV circle", Vars::Aimbot::General::FOVCircle);
    });

    ImGui::SameLine();
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

    CustomGUI::Child("Crits", { 230.0f, 150.0f }, []()
    {
        CustomGUI::Checkbox("Force crits", Vars::CritHack::ForceCrits);
        CustomGUI::Checkbox("Avoid random crits", Vars::CritHack::AvoidRandomCrits);
        CustomGUI::Checkbox("Always melee crit", Vars::CritHack::AlwaysMeleeCrit);
    });
}

void CCustomMenu::DrawLegitRage()
{
    CustomGUI::Child("Hitscan", { 230.0f, 250.0f }, []()
    {
        CustomGUI::MultiCombo("Hitboxes", Vars::Aimbot::Hitscan::Hitboxes);
        CustomGUI::MultiCombo("Multipoint", Vars::Aimbot::Hitscan::MultipointHitboxes);
        CustomGUI::MultiCombo("Modifiers", Vars::Aimbot::Hitscan::Modifiers);
        CustomGUI::SliderFloat("Multipoint scale", Vars::Aimbot::Hitscan::MultipointScale);
        CustomGUI::SliderFloat("Tapfire distance", Vars::Aimbot::Hitscan::TapfireDistance);
    });

    ImGui::SameLine();
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

    CustomGUI::Child("Melee", { 230.0f, 150.0f }, []()
    {
        CustomGUI::Checkbox("Auto backstab", Vars::Aimbot::Melee::AutoBackstab);
        CustomGUI::Checkbox("Ignore razorback", Vars::Aimbot::Melee::IgnoreRazorback);
        CustomGUI::Checkbox("Swing prediction", Vars::Aimbot::Melee::SwingPrediction);
        CustomGUI::Checkbox("Whip team", Vars::Aimbot::Melee::WhipTeam);
    });
}

void CCustomMenu::DrawLegitTrigger()
{
    CustomGUI::Child("Projectile", { 230.0f, 310.0f }, []()
    {
        CustomGUI::MultiCombo("Strafe prediction", Vars::Aimbot::Projectile::StrafePrediction);
        CustomGUI::Combo("Splash prediction", Vars::Aimbot::Projectile::SplashPrediction);
        CustomGUI::MultiCombo("Auto detonate", Vars::Aimbot::Projectile::AutoDetonate);
        CustomGUI::MultiCombo("Auto airblast", Vars::Aimbot::Projectile::AutoAirblast);
        CustomGUI::MultiCombo("Hitboxes", Vars::Aimbot::Projectile::Hitboxes);
        CustomGUI::SliderFloat("Max sim time", Vars::Aimbot::Projectile::MaxSimulationTime);
        CustomGUI::SliderFloat("Hit chance", Vars::Aimbot::Projectile::HitChance);
    });

    ImGui::SameLine();
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

    CustomGUI::Child("Backtrack", { 230.0f, 150.0f }, []()
    {
        CustomGUI::SliderInt("Latency", Vars::Backtrack::Latency);
        CustomGUI::SliderInt("Interp", Vars::Backtrack::Interp);
        CustomGUI::SliderInt("Window", Vars::Backtrack::Window);
    });
}

void CCustomMenu::DrawVisualsTab()
{
    switch (m_iCurrentSubTab)
    {
    case 0: DrawVisualsEntities(); break;
    case 1: DrawVisualsGame(); break;
    case 2: DrawVisualsScreen(); break;
    }
}

void CCustomMenu::DrawVisualsEntities()
{
    CustomGUI::Child("ESP", { 230.0f, 310.0f }, []()
    {
        // ESP settings would go here
    });
}

void CCustomMenu::DrawVisualsGame()
{
    CustomGUI::Child("World", { 230.0f, 200.0f }, []()
    {
        CustomGUI::Checkbox("Thirdperson", Vars::Visuals::Thirdperson::Enabled);
        CustomGUI::Checkbox("Crosshair", Vars::Visuals::Thirdperson::Crosshair);
        CustomGUI::SliderFloat("Distance", Vars::Visuals::Thirdperson::Distance);
        CustomGUI::SliderFloat("Right", Vars::Visuals::Thirdperson::Right);
        CustomGUI::SliderFloat("Up", Vars::Visuals::Thirdperson::Up);
    });

    ImGui::SameLine();
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

    CustomGUI::Child("UI", { 230.0f, 200.0f }, []()
    {
        CustomGUI::Combo("Streamer mode", Vars::Visuals::UI::StreamerMode);
        CustomGUI::SliderFloat("Field of view", Vars::Visuals::UI::FieldOfView);
        CustomGUI::SliderFloat("Zoomed FOV", Vars::Visuals::UI::ZoomFieldOfView);
        CustomGUI::SliderFloat("Aspect ratio", Vars::Visuals::UI::AspectRatio);
        CustomGUI::Checkbox("Reveal scoreboard", Vars::Visuals::UI::RevealScoreboard);
        CustomGUI::Checkbox("Scoreboard utility", Vars::Visuals::UI::ScoreboardUtility);
    });
}

void CCustomMenu::DrawVisualsScreen()
{
    CustomGUI::Child("Removals", { 230.0f, 310.0f }, []()
    {
        CustomGUI::Checkbox("Interpolation", Vars::Visuals::Removals::Interpolation);
        CustomGUI::Checkbox("Lerp", Vars::Visuals::Removals::Lerp);
        CustomGUI::Checkbox("Disguises", Vars::Visuals::Removals::Disguises);
        CustomGUI::Checkbox("Taunts", Vars::Visuals::Removals::Taunts);
        CustomGUI::Checkbox("Scope", Vars::Visuals::Removals::Scope);
        CustomGUI::Checkbox("Post processing", Vars::Visuals::Removals::PostProcessing);
        CustomGUI::Checkbox("Screen overlays", Vars::Visuals::Removals::ScreenOverlays);
        CustomGUI::Checkbox("Screen effects", Vars::Visuals::Removals::ScreenEffects);
        CustomGUI::Checkbox("View punch", Vars::Visuals::Removals::ViewPunch);
        CustomGUI::Checkbox("Angle forcing", Vars::Visuals::Removals::AngleForcing);
        CustomGUI::Checkbox("Ragdolls", Vars::Visuals::Removals::Ragdolls);
        CustomGUI::Checkbox("Gibs", Vars::Visuals::Removals::Gibs);
        CustomGUI::Checkbox("MOTD", Vars::Visuals::Removals::MOTD);
    });
}

void CCustomMenu::DrawMiscTab()
{
    switch (m_iCurrentSubTab)
    {
    case 0: DrawMiscGeneral(); break;
    case 1: // HVH tab
        CustomGUI::Child("Anti-Aim", { 230.0f, 310.0f }, []()
        {
            CustomGUI::Checkbox("Enabled", Vars::AntiAim::Enabled);
            CustomGUI::Combo("Real pitch", Vars::AntiAim::PitchReal);
            CustomGUI::Combo("Fake pitch", Vars::AntiAim::PitchFake);
            CustomGUI::Combo("Real yaw", Vars::AntiAim::YawReal);
            CustomGUI::Combo("Fake yaw", Vars::AntiAim::YawFake);
            CustomGUI::SliderFloat("Real offset", Vars::AntiAim::RealYawOffset);
            CustomGUI::SliderFloat("Fake offset", Vars::AntiAim::FakeYawOffset);
            CustomGUI::Checkbox("Minwalk", Vars::AntiAim::MinWalk);
            CustomGUI::Checkbox("Anti-overlap", Vars::AntiAim::AntiOverlap);
        });

        ImGui::SameLine();
        ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

        CustomGUI::Child("Resolver", { 230.0f, 200.0f }, []()
        {
            CustomGUI::Checkbox("Enabled", Vars::Resolver::Enabled);
            CustomGUI::Checkbox("Auto resolve", Vars::Resolver::AutoResolve);
            CustomGUI::Checkbox("Cheaters only", Vars::Resolver::AutoResolveCheatersOnly);
            CustomGUI::SliderFloat("Yaw amount", Vars::Resolver::AutoResolveYawAmount);
            CustomGUI::SliderFloat("Pitch amount", Vars::Resolver::AutoResolvePitchAmount);
        });
        break;
    }
}

void CCustomMenu::DrawMiscGeneral()
{
    CustomGUI::Child("General", { 230.0f, 310.0f }, []()
    {
        CustomGUI::Checkbox("Doubletap", Vars::Doubletap::Doubletap);
        CustomGUI::Checkbox("Warp", Vars::Doubletap::Warp);
        CustomGUI::Checkbox("Recharge ticks", Vars::Doubletap::RechargeTicks);
        CustomGUI::Checkbox("Anti-warp", Vars::Doubletap::AntiWarp);
        CustomGUI::SliderInt("Tick limit", Vars::Doubletap::TickLimit);
        CustomGUI::SliderInt("Warp rate", Vars::Doubletap::WarpRate);
    });

    ImGui::SameLine();
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

    CustomGUI::Child("Fakelag", { 230.0f, 200.0f }, []()
    {
        CustomGUI::Combo("Fakelag", Vars::Fakelag::Fakelag);
        CustomGUI::MultiCombo("Options", Vars::Fakelag::Options);
        CustomGUI::SliderInt("Plain ticks", Vars::Fakelag::PlainTicks);
        CustomGUI::Checkbox("Unchoke on attack", Vars::Fakelag::UnchokeOnAttack);
        CustomGUI::Checkbox("Retain blastjump", Vars::Fakelag::RetainBlastJump);
    });
}

void CCustomMenu::DrawMovementTab()
{
    switch (m_iCurrentSubTab)
    {
    case 0: DrawMovementGeneral(); break;
    case 1: // Edgebug tab
        CustomGUI::Child("Edge Bug", { 230.0f, 200.0f }, []()
        {
            // Edge bug settings from Misc
        });
        break;
    }
}

void CCustomMenu::DrawMovementGeneral()
{
    CustomGUI::Child("Movement", { 230.0f, 310.0f }, []()
    {
        CustomGUI::Checkbox("Auto peek", Vars::AutoPeek::Enabled);
        CustomGUI::Checkbox("Speedhack", Vars::Speedhack::Enabled);
        CustomGUI::SliderInt("Speed amount", Vars::Speedhack::Amount);
    });
}

void CCustomMenu::DrawSkinsTab()
{
    CustomGUI::Child("Skins", { 230.0f, 310.0f }, []()
    {
        // Skin changer settings
    });
}

void CCustomMenu::DrawConfigTab()
{
    switch (m_iCurrentSubTab)
    {
    case 0: DrawConfigGeneral(); break;
    case 1: // Binds tab
        CustomGUI::Child("Binds", { 460.0f, 400.0f }, []()
        {
            // Binds list
        });
        break;
    }
}

void CCustomMenu::DrawConfigGeneral()
{
    CustomGUI::Child("Config", { 230.0f, 310.0f }, []()
    {
        static char szConfigName[32] = "";
        CustomGUI::InputText("Config name", "", szConfigName, sizeof(szConfigName), 180.0f, 25.0f);
        
        if (CustomGUI::Button("Save"))
        {
            // Save config
        }
        if (CustomGUI::Button("Load"))
        {
            // Load config
        }
        if (CustomGUI::Button("Delete"))
        {
            // Delete config
        }
    });

    ImGui::SameLine();
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + 10.0f);

    CustomGUI::Child("Menu", { 230.0f, 200.0f }, []()
    {
        CustomGUI::InputText("Cheat title", Vars::Menu::CheatTitle, 180.0f);
        CustomGUI::InputText("Cheat tag", Vars::Menu::CheatTag, 180.0f);
        CustomGUI::SliderFloat("Scale", Vars::Menu::Scale);
        CustomGUI::Checkbox("Cheap text", Vars::Menu::CheapText);
        CustomGUI::Checkbox("Bind window", Vars::Menu::BindWindow);
    });
}

void CCustomMenu::DrawPlayersTab()
{
    CustomGUI::Child("Player List", { 460.0f, 400.0f }, []()
    {
        // Player list would go here
    });
}

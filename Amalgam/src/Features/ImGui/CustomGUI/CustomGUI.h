#pragma once
#include "../../../SDK/SDK.h"
#include "../Render.h"
#include <ImGui/imgui.h>
#include <ImGui/imgui_internal.h>
#include <functional>
#include <vector>
#include <string>

// Forward declarations
template <class T>
class ConfigVar;

namespace CustomGUI
{
    // Fonts for custom GUI
    namespace Fonts
    {
        inline ImFont* Regular = nullptr;
        inline ImFont* Bold = nullptr;
        inline ImFont* Title = nullptr;
        inline ImFont* Icons = nullptr;
        
        void Initialize();
    }

    // Color scheme
    namespace Colors
    {
        inline ImColor Background = ImColor(7, 7, 7, 255);
        inline ImColor BackgroundDark = ImColor(10, 10, 10, 255);
        inline ImColor Border = ImColor(25, 25, 25, 255);
        inline ImColor Accent = ImColor(230, 210, 255, 255);
        inline ImColor Text = ImColor(255, 255, 255, 255);
        inline ImColor TextDisabled = ImColor(180, 180, 180, 255);
        
        void LoadFromVars();
    }

    // Tab structure
    struct Tab_t
    {
        std::string sName;
        std::string sIcon;
        std::vector<std::string> vSubTabs;
    };

    // Keybind structure
    enum EKeyStyle
    {
        AlwaysOn = 0,
        Hold,
        Toggle
    };

    struct KeyBind_t
    {
        int iKey = 0;
        int iStyle = 0;
    };

    // Helper functions
    float CalculatePopupHeight(int iMaxItems);
    std::string ConstructMultiList(const std::vector<std::string>& vList);

    // Custom elements
    void Tabs(std::vector<Tab_t>& vTabs, int& iCurrentTab, int& iCurrentSubTab);
    void Child(const char* sName, const ImVec2& vSize, const std::function<void()>& fnContent);
    
    bool Checkbox(const char* sName, bool* pValue, bool bDisabled = false);
    bool Checkbox(const char* sName, ConfigVar<bool>& var, bool bDisabled = false);
    
    bool Button(const char* sName, const ImVec2& vSize = { 180.0f, 25.0f });
    
    bool SliderFloat(const char* sName, float* pValue, float flMin, float flMax, const char* sFormat = "%.2f");
    bool SliderFloat(const char* sName, ConfigVar<float>& var, const char* sFormat = nullptr);
    bool SliderInt(const char* sName, int* pValue, int iMin, int iMax, const char* sFormat = "%d");
    bool SliderInt(const char* sName, ConfigVar<int>& var, const char* sFormat = nullptr);
    
    bool Combo(const char* sName, int* pSelected, const char* const* pItems, int iItemCount);
    bool Combo(const char* sName, ConfigVar<int>& var);
    bool MultiCombo(const char* sName, bool* pValues, const char* const* pItems, int iItemCount);
    bool MultiCombo(const char* sName, ConfigVar<int>& var);
    
    bool InputText(const char* sName, const char* sHint, char* pBuffer, size_t iBufferSize, float flWidth = 180.0f, float flHeight = 25.0f, ImGuiInputTextFlags flags = 0);
    bool InputText(const char* sName, ConfigVar<std::string>& var, float flWidth = 180.0f);
    
    bool KeyBind(const char* sName, KeyBind_t* pKeyBind, bool bShowStyle = true);
    
    void Spinner(const char* sName);
}

#include "CustomGUI.h"
#include "../Render.h"

namespace CustomGUI
{
    namespace Fonts
    {
        void Initialize()
        {
            Regular = F::Render.FontRegular;
            Bold = F::Render.FontBold;
            Title = F::Render.FontLarge;
            Icons = F::Render.IconFont;
        }
    }

    namespace Colors
    {
        void LoadFromVars()
        {
            auto ColorToImColor = [](Color_t tColor) -> ImColor {
                return ImColor(tColor.r / 255.f, tColor.g / 255.f, tColor.b / 255.f, tColor.a / 255.f);
            };

            Accent = ColorToImColor(Vars::Menu::Theme::Accent.Value);
            Text = ColorToImColor(Vars::Menu::Theme::Active.Value);
            TextDisabled = ColorToImColor(Vars::Menu::Theme::Inactive.Value);
        }
    }

    void Tabs(std::vector<Tab_t>& vTabs, int& iCurrentTab, int& iCurrentSubTab)
    {
        using namespace ImGui;
        
        float flYOffsetAdjustment = 0.0f;
        const float flSpacingLeft = 11.0f;
        const float flSpacingTop = 125.0f;
        const float flWidth = 141.0f;
        const float flHeight = 25.0f;
        const float flTextSpacing = 25.0f;
        const float flRounding = 4.0f;

        ImFont* pRegular = Fonts::Regular ? Fonts::Regular : GetFont();
        ImFont* pIcons = Fonts::Icons ? Fonts::Icons : GetFont();
        ImVec2 vWindowPos = GetWindowPos();

        for (size_t i = 0; i < vTabs.size(); ++i)
        {
            float flXOffset = vWindowPos.x + flSpacingLeft + flTextSpacing;
            float flYOffset = vWindowPos.y + flSpacingTop + (i * flHeight) + 5 + flYOffsetAdjustment;
            ImVec2 vTextPos = { flXOffset, flYOffset };

            ImVec2 vTabMin = { vWindowPos.x + flSpacingLeft, vWindowPos.y + flSpacingTop + i * flHeight + flYOffsetAdjustment };
            ImVec2 vTabMax = { vWindowPos.x + flWidth, vWindowPos.y + flSpacingTop + (i + 1) * flHeight + flYOffsetAdjustment };

            bool bHovered = IsMouseHoveringRect(vTabMin, vTabMax);
            bool bSelected = (static_cast<int>(i) == iCurrentTab);

            if (bSelected)
                GetWindowDrawList()->AddRectFilled(vTabMin, vTabMax, Colors::Border, flRounding, ImDrawFlags_RoundCornersTop);

            GetWindowDrawList()->AddText(pRegular, pRegular->FontSize, vTextPos, Colors::Text, vTabs[i].sName.c_str());
            
            if (!vTabs[i].sIcon.empty())
            {
                ImVec2 vIconPos = { vTextPos.x - 20.0f, vTextPos.y };
                GetWindowDrawList()->AddText(pIcons, pIcons->FontSize, vIconPos, Colors::Accent, vTabs[i].sIcon.c_str());
            }

            if (bHovered && IsMouseClicked(ImGuiMouseButton_Left))
            {
                iCurrentTab = static_cast<int>(i);
                iCurrentSubTab = 0;
            }

            if (bSelected && !vTabs[i].vSubTabs.empty())
            {
                ImVec2 vGroupMin = { vWindowPos.x + flSpacingLeft, vWindowPos.y + flSpacingTop + (i + 1) * flHeight + flYOffsetAdjustment };
                ImVec2 vGroupMax = { vWindowPos.x + flWidth, vWindowPos.y + flSpacingTop + (i + 1 + vTabs[i].vSubTabs.size()) * flHeight + flYOffsetAdjustment };

                GetWindowDrawList()->AddRectFilled(vGroupMin, vGroupMax, Colors::BackgroundDark, flRounding, ImDrawFlags_RoundCornersBottom);

                for (size_t j = 0; j < vTabs[i].vSubTabs.size(); ++j)
                {
                    ImVec2 vSubTextPos = { flXOffset, flYOffset + (j + 1) * flHeight };

                    bool bSubSelected = (iCurrentSubTab == static_cast<int>(j));
                    ImVec2 vSubTabMin = { vGroupMin.x, vGroupMin.y + j * flHeight };
                    ImVec2 vSubTabMax = { vGroupMax.x, vGroupMin.y + (j + 1) * flHeight };
                    bool bSubHovered = IsMouseHoveringRect(vSubTabMin, vSubTabMax);

                    GetWindowDrawList()->AddText(pRegular, pRegular->FontSize, vSubTextPos, 
                        bSubSelected ? Colors::Accent : Colors::Text, vTabs[i].vSubTabs[j].c_str());

                    if (bSubHovered && IsMouseClicked(ImGuiMouseButton_Left))
                        iCurrentSubTab = static_cast<int>(j);
                }

                flYOffsetAdjustment += vTabs[i].vSubTabs.size() * flHeight;
            }
        }
    }

    void Child(const char* sName, const ImVec2& vSize, const std::function<void()>& fnContent)
    {
        using namespace ImGui;
        
        const float flTitleBarHeight = 30.0f;
        const float flPadding = 10.0f;
        const float flRounding = 4.0f;

        ImFont* pBold = Fonts::Bold ? Fonts::Bold : GetFont();

        // Use standard ImGui child with proper flags
        PushStyleColor(ImGuiCol_ChildBg, ImVec4(0.04f, 0.04f, 0.04f, 1.0f)); // BackgroundDark
        PushStyleColor(ImGuiCol_Border, ImVec4(0.1f, 0.1f, 0.1f, 1.0f)); // Border
        PushStyleVar(ImGuiStyleVar_ChildRounding, flRounding);
        PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(flPadding, flPadding));
        
        if (BeginChild(sName, vSize, ImGuiChildFlags_Borders, ImGuiWindowFlags_None))
        {
            // Draw title bar
            ImVec2 vPos = GetWindowPos();
            ImVec2 vTitleEnd = { vPos.x + vSize.x, vPos.y + flTitleBarHeight };
            ImVec2 vTextPos = { vPos.x + flPadding, vPos.y + flTitleBarHeight / 4 };
            
            GetWindowDrawList()->AddRectFilled(vPos, vTitleEnd, IM_COL32(10, 10, 10, 255), flRounding, ImDrawFlags_RoundCornersTop);
            GetWindowDrawList()->AddLine({ vPos.x, vPos.y + flTitleBarHeight }, { vPos.x + vSize.x, vPos.y + flTitleBarHeight }, IM_COL32(25, 25, 25, 255));
            GetWindowDrawList()->AddText(pBold, pBold->FontSize, vTextPos, IM_COL32(255, 255, 255, 255), sName);

            // Set cursor for content
            SetCursorPosY(flTitleBarHeight + flPadding);
            
            // Execute content
            fnContent();
        }
        EndChild();
        
        PopStyleVar(2);
        PopStyleColor(2);
    }

    // Simple checkbox using standard ImGui
    bool Checkbox(const char* sName, bool* pValue, bool bDisabled)
    {
        using namespace ImGui;
        
        if (bDisabled)
        {
            PushStyleVar(ImGuiStyleVar_Alpha, 0.5f);
            bool temp = *pValue;
            ImGui::Checkbox(sName, &temp);
            PopStyleVar();
            return false;
        }
        
        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_FrameBgActive, IM_COL32(20, 20, 20, 255));
        PushStyleColor(ImGuiCol_CheckMark, IM_COL32(230, 210, 255, 255));
        
        bool bResult = ImGui::Checkbox(sName, pValue);
        
        PopStyleColor(4);
        return bResult;
    }

    bool Checkbox(const char* sName, ConfigVar<bool>& var, bool bDisabled)
    {
        return Checkbox(sName, &var.Value, bDisabled);
    }

    bool Button(const char* sName, const ImVec2& vSize)
    {
        using namespace ImGui;
        
        PushStyleColor(ImGuiCol_Button, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_ButtonHovered, IM_COL32(40, 40, 40, 255));
        PushStyleColor(ImGuiCol_ButtonActive, IM_COL32(30, 30, 30, 255));
        
        bool bResult = ImGui::Button(sName, vSize);
        
        PopStyleColor(3);
        return bResult;
    }

    bool SliderFloat(const char* sName, float* pValue, float flMin, float flMax, const char* sFormat)
    {
        using namespace ImGui;
        
        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_FrameBgActive, IM_COL32(20, 20, 20, 255));
        PushStyleColor(ImGuiCol_SliderGrab, IM_COL32(230, 210, 255, 255));
        PushStyleColor(ImGuiCol_SliderGrabActive, IM_COL32(230, 210, 255, 255));
        
        PushItemWidth(180.0f);
        bool bResult = ImGui::SliderFloat(sName, pValue, flMin, flMax, sFormat);
        PopItemWidth();
        
        PopStyleColor(5);
        return bResult;
    }

    bool SliderFloat(const char* sName, ConfigVar<float>& var, const char* sFormat)
    {
        const char* sFmt = sFormat ? sFormat : (var.m_sExtra ? var.m_sExtra : "%.2f");
        return SliderFloat(sName, &var.Value, var.m_unMin.f, var.m_unMax.f, sFmt);
    }

    bool SliderInt(const char* sName, int* pValue, int iMin, int iMax, const char* sFormat)
    {
        using namespace ImGui;
        
        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_FrameBgActive, IM_COL32(20, 20, 20, 255));
        PushStyleColor(ImGuiCol_SliderGrab, IM_COL32(230, 210, 255, 255));
        PushStyleColor(ImGuiCol_SliderGrabActive, IM_COL32(230, 210, 255, 255));
        
        PushItemWidth(180.0f);
        bool bResult = ImGui::SliderInt(sName, pValue, iMin, iMax, sFormat);
        PopItemWidth();
        
        PopStyleColor(5);
        return bResult;
    }

    bool SliderInt(const char* sName, ConfigVar<int>& var, const char* sFormat)
    {
        const char* sFmt = sFormat ? sFormat : (var.m_sExtra ? var.m_sExtra : "%d");
        return SliderInt(sName, &var.Value, var.m_unMin.i, var.m_unMax.i, sFmt);
    }

    bool Combo(const char* sName, int* pSelected, const char* const* pItems, int iItemCount)
    {
        using namespace ImGui;
        
        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_PopupBg, IM_COL32(15, 15, 15, 255));
        PushStyleColor(ImGuiCol_Header, IM_COL32(230, 210, 255, 100));
        PushStyleColor(ImGuiCol_HeaderHovered, IM_COL32(50, 50, 50, 255));
        
        PushItemWidth(180.0f);
        bool bResult = ImGui::Combo(sName, pSelected, pItems, iItemCount);
        PopItemWidth();
        
        PopStyleColor(5);
        return bResult;
    }

    bool Combo(const char* sName, ConfigVar<int>& var)
    {
        using namespace ImGui;
        
        if (var.m_vValues.empty())
            return false;

        std::vector<const char*> vItems;
        for (const auto& sItem : var.m_vValues)
        {
            if (strcmp(sItem, "##Divider") != 0)
                vItems.push_back(sItem);
        }

        if (vItems.empty())
            return false;

        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_PopupBg, IM_COL32(15, 15, 15, 255));
        PushStyleColor(ImGuiCol_Header, IM_COL32(230, 210, 255, 100));
        PushStyleColor(ImGuiCol_HeaderHovered, IM_COL32(50, 50, 50, 255));
        
        PushItemWidth(180.0f);
        
        int iSelected = var.Value;
        bool bResult = false;
        
        if (BeginCombo(sName, vItems[iSelected < static_cast<int>(vItems.size()) ? iSelected : 0]))
        {
            for (size_t i = 0; i < vItems.size(); i++)
            {
                bool bIsSelected = (iSelected == static_cast<int>(i));
                if (Selectable(vItems[i], bIsSelected))
                {
                    var.Value = static_cast<int>(i);
                    bResult = true;
                }
                if (bIsSelected)
                    SetItemDefaultFocus();
            }
            EndCombo();
        }
        
        PopItemWidth();
        PopStyleColor(5);
        return bResult;
    }

    static std::string ConstructMultiList(const std::vector<std::string>& vList)
    {
        std::string sResult;
        for (const auto& sItem : vList)
        {
            if (sResult.length() >= 20)
            {
                sResult.append(" ...");
                break;
            }
            if (!sItem.empty())
            {
                if (!sResult.empty())
                    sResult.append(", ");
                sResult.append(sItem);
            }
        }
        if (sResult.empty())
            sResult = "none";
        return sResult;
    }

    bool MultiCombo(const char* sName, bool* pValues, const char* const* pItems, int iItemCount)
    {
        using namespace ImGui;
        
        std::vector<std::string> vSelected;
        for (int i = 0; i < iItemCount; i++)
        {
            if (pValues[i])
                vSelected.push_back(pItems[i]);
        }

        std::string sPreview = ConstructMultiList(vSelected);

        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_PopupBg, IM_COL32(15, 15, 15, 255));
        PushStyleColor(ImGuiCol_Header, IM_COL32(230, 210, 255, 100));
        PushStyleColor(ImGuiCol_HeaderHovered, IM_COL32(50, 50, 50, 255));
        
        PushItemWidth(180.0f);
        
        bool bResult = false;
        if (BeginCombo(sName, sPreview.c_str()))
        {
            for (int i = 0; i < iItemCount; i++)
            {
                if (Selectable(pItems[i], pValues[i], ImGuiSelectableFlags_DontClosePopups))
                {
                    pValues[i] = !pValues[i];
                    bResult = true;
                }
            }
            EndCombo();
        }
        
        PopItemWidth();
        PopStyleColor(5);
        return bResult;
    }

    bool MultiCombo(const char* sName, ConfigVar<int>& var)
    {
        using namespace ImGui;
        
        if (var.m_vValues.empty())
            return false;

        std::vector<std::string> vSelected;
        std::vector<const char*> vItems;
        
        for (size_t i = 0; i < var.m_vValues.size(); i++)
        {
            if (strcmp(var.m_vValues[i], "##Divider") != 0)
            {
                vItems.push_back(var.m_vValues[i]);
                if (var.Value & (1 << i))
                    vSelected.push_back(var.m_vValues[i]);
            }
        }

        std::string sPreview = ConstructMultiList(vSelected);

        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_PopupBg, IM_COL32(15, 15, 15, 255));
        PushStyleColor(ImGuiCol_Header, IM_COL32(230, 210, 255, 100));
        PushStyleColor(ImGuiCol_HeaderHovered, IM_COL32(50, 50, 50, 255));
        
        PushItemWidth(180.0f);
        
        bool bResult = false;
        if (BeginCombo(sName, sPreview.c_str()))
        {
            for (size_t i = 0; i < var.m_vValues.size(); i++)
            {
                if (strcmp(var.m_vValues[i], "##Divider") == 0)
                    continue;

                bool bSelected = (var.Value & (1 << i)) != 0;

                if (Selectable(var.m_vValues[i], bSelected, ImGuiSelectableFlags_DontClosePopups))
                {
                    if (bSelected)
                        var.Value &= ~(1 << i);
                    else
                        var.Value |= (1 << i);
                    bResult = true;
                }
            }
            EndCombo();
        }
        
        PopItemWidth();
        PopStyleColor(5);
        return bResult;
    }

    bool InputText(const char* sName, const char* sHint, char* pBuffer, size_t iBufferSize, float flWidth, float flHeight, ImGuiInputTextFlags flags)
    {
        using namespace ImGui;
        
        PushStyleColor(ImGuiCol_FrameBg, IM_COL32(25, 25, 25, 255));
        PushStyleColor(ImGuiCol_FrameBgHovered, IM_COL32(35, 35, 35, 255));
        PushStyleColor(ImGuiCol_FrameBgActive, IM_COL32(20, 20, 20, 255));
        
        PushItemWidth(flWidth);
        bool bResult = ImGui::InputTextWithHint(sName, sHint, pBuffer, iBufferSize, flags);
        PopItemWidth();
        
        PopStyleColor(3);
        return bResult;
    }

    bool InputText(const char* sName, ConfigVar<std::string>& var, float flWidth)
    {
        char szBuffer[256];
        strncpy_s(szBuffer, var.Value.c_str(), sizeof(szBuffer) - 1);
        
        if (InputText(sName, "", szBuffer, sizeof(szBuffer), flWidth, 25.0f, ImGuiInputTextFlags_None))
        {
            var.Value = szBuffer;
            return true;
        }
        return false;
    }

    bool KeyBind(const char* sName, KeyBind_t* pKeyBind, bool bShowStyle)
    {
        // Simplified keybind - just show the key name for now
        using namespace ImGui;
        
        static const char* g_KeyNames[] = {
            "-",   "m1",  "m2",  "cn",  "m3",  "m4",  "m5",  "-",   "bac", "tab", "-",   "-",   "clr", "ret", "-",   "-",
            "shi", "ctl", "men", "pau", "cap", "kan", "-",   "jun", "fin", "kan", "-",   "esc", "con", "nco", "acc", "mad",
            "spa", "pgu", "pgd", "end", "hom", "lef", "up",  "rig", "dow", "sel", "pri", "exe", "pri", "ins", "del", "hel",
            "0",   "1",   "2",   "3",   "4",   "5",   "6",   "7",   "8",   "9",   "-",   "-",   "-",   "-",   "-",   "-",
            "-",   "a",   "b",   "c",   "d",   "e",   "f",   "g",   "h",   "i",   "j",   "k",   "l",   "m",   "n",   "o",
            "p",   "q",   "r",   "s",   "t",   "u",   "v",   "w",   "x",   "y",   "z",   "win", "win", "app", "-",   "sle",
            "num", "num", "num", "num", "num", "num", "num", "num", "num", "num", "mul", "add", "sep", "min", "dec", "div",
            "f1",  "f2",  "f3",  "f4",  "f5",  "f6",  "f7",  "f8",  "f9",  "f10", "f11", "f12", "f13", "f14", "f15", "f16",
            "f17", "f18", "f19", "f20", "f21", "f22", "f23", "f24", "-",   "-",   "-",   "-",   "-",   "-",   "-",   "-",
            "num", "scr", "equ", "mas", "toy", "oya", "oya", "-",   "-",   "-",   "-",   "-",   "-",   "-",   "-",   "-",
            "shi", "shi", "ctr", "ctr", "alt", "alt"
        };
        
        const char* szKeyName = (pKeyBind->iKey > 0 && pKeyBind->iKey < 166) ? g_KeyNames[pKeyBind->iKey] : "none";
        
        Text("%s: [%s]", sName, szKeyName);
        return false;
    }

    void Spinner(const char* sName)
    {
        using namespace ImGui;
        Text("Loading %s...", sName);
    }
}

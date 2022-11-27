module Icon exposing (Icon(..), iconClass, view)

import Html
import Html.Attributes as AT


type Icon
    = Wifi
    | Exclamation
    | Circle
    | CircleFull
    | ChevronRight
    | ChevronLeft
    | LightOn
    | LightOff
    | CircleDot
    | Plane
    | Video
    | File
    | Configuration
    | Camera
    | Terminal


iconClass : Icon -> String
iconClass kind =
    case kind of
        Terminal ->
            "icon fa-solid fa-terminal"

        ChevronLeft ->
            "icon fa-solid fa-chevron-left"

        ChevronRight ->
            "icon fa-solid fa-chevron-right"

        CircleFull ->
            "icon fa-solid fa-circle"

        Circle ->
            "icon fa-regular fa-circle"

        Wifi ->
            "icon fa-solid fa-wifi"

        Exclamation ->
            "icon fa-solid fa-exclamation"

        Video ->
            "fa-solid fa-video"

        Plane ->
            "fa-solid fa-paper-plane"

        Camera ->
            "fa-solid fa-camera"

        File ->
            "fa-solid fa-file"

        CircleDot ->
            "fa-solid fa-circle-dot"

        LightOn ->
            "fa-solid fa-lightbulb"

        LightOff ->
            "fa-regular fa-lightbulb"

        Configuration ->
            "fa-solid fa-gear"


view : Icon -> Html.Html m
view icon =
    Html.div [ AT.class (iconClass icon) ] []

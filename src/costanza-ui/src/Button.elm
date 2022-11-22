module Button exposing (..)

import Html
import Html.Attributes as AT
import Html.Events as EV


type ButtonIcon
    = LightOn
    | LightOff
    | CircleDot
    | Plane
    | Video
    | File
    | Camera


type Button a
    = Icon ButtonIcon a
    | Text String a


type RGBColor
    = Red
    | Green
    | Blue


type ButtonVariant
    = Disabled
    | Primary
    | Secondary
    | Warning
    | RGB RGBColor


disabledOr : Bool -> ButtonVariant -> ButtonVariant
disabledOr isDisabled other =
    if isDisabled then
        Disabled

    else
        other


view : ( Button a, ButtonVariant ) -> Html.Html a
view kind =
    let
        isDisabled =
            Tuple.second kind == Disabled

        variantClass =
            case Tuple.second kind of
                RGB Red ->
                    "bg-red"

                RGB Green ->
                    "bg-green"

                RGB Blue ->
                    "bg-blue"

                Primary ->
                    "button-primary"

                Secondary ->
                    "button-primary"

                Warning ->
                    "button-warning"

                Disabled ->
                    "button-disabled"
    in
    Html.div []
        [ case Tuple.first kind of
            Text content message ->
                Html.button
                    [ AT.class variantClass
                    , EV.onClick message
                    , AT.disabled isDisabled
                    ]
                    [ Html.text content ]

            Icon ico message ->
                Html.button
                    [ AT.class ("icon-button " ++ variantClass)
                    , EV.onClick message
                    , AT.disabled isDisabled
                    ]
                    [ Html.i [ AT.class (iconClass ico) ] [] ]
        ]


iconClass : ButtonIcon -> String
iconClass kind =
    case kind of
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

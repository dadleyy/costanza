module Button exposing (..)

import Html
import Html.Attributes as AT
import Html.Events as EV
import Icon


type alias ButtonIcon =
    Icon.Icon


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
                    [ Html.i [ AT.class (Icon.iconClass ico) ] [] ]
        ]

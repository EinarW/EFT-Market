#from os import path
import gspread
import json
from datetime import datetime, timedelta
from oauth2client.service_account import ServiceAccountCredentials

scope = ['https://spreadsheets.google.com/feeds', 'https://www.googleapis.com/auth/drive']
credentials = ServiceAccountCredentials.from_json_keyfile_name('eft-sheet-updater-96373fea2213.json', scope)
gc = gspread.authorize(credentials)
spreadsheet = gc.open_by_key('1YVWW8bBsGZthcil8HRrrt0uwYO6h5YXVGaC_z0Ety4A')
api_ref = spreadsheet.get_worksheet(3)
detailed = spreadsheet.get_worksheet(2)
ids_to_update = api_ref.col_values(3) # All values from ID column (3)


# Averages
cells_to_update = api_ref.range('D2:D300')
with open('averages.json') as json_file:
    averages = json.load(json_file)

    new_values = []
    index = 0
    for id in ids_to_update:
        if index is not 0:          # Skip title row
            new_val = averages.get(id)
            new_values.append(new_val)

        index += 1

    index = 0
    for cell in cells_to_update:
        if index < len(new_values):
            cell.value = new_values[index]
            index += 1
        else:
            break

    # Update in batch
    api_ref.update_cells(cells_to_update)


# Base prices
cells_to_update = api_ref.range('E2:E300')
with open('prices.json') as json_file:
    prices = json.load(json_file)

    new_values = []
    index = 0
    for id in ids_to_update:
        if index is not 0:          # Skip title row
            new_val = prices.get(id)
            new_values.append(new_val)

        index += 1

    index = 0
    for cell in cells_to_update:
        if index < len(new_values):
            cell.value = new_values[index]
            index += 1
        else:
            break

    # Update in batch
    api_ref.update_cells(cells_to_update)


# Update title with timestamp
date = datetime.utcnow().strftime('%Y-%m-%d %H:%M')
nextdate = (datetime.utcnow() + timedelta(minutes=10)).strftime('%Y-%m-%d %H:%M')

title = 'Last updated: {} UTC.  Next update approx. {} UTC.'.format(date, nextdate)
detailed.update_acell('C10', title)



